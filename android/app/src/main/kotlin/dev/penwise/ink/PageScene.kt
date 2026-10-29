package dev.penwise.ink

import dev.penwise.core.Native
import java.util.concurrent.CopyOnWriteArrayList

/**
 * Told after a [PageScene] tool applied its edit to the scene *and* queued it on the sync core (one native call, one
 * lock acquisition, so a remote refresh can never land between the two): time to persist and push.
 */
fun interface EditSink {
    fun committed()
}

/** Layers of the page, as the bit set the scene queries take. */
object Layers {
    const val INK = 1
    const val AGENT = 2
    const val TEXT = 4
    const val ALL = INK or AGENT or TEXT

    /** What the pen eraser touches: ink and agent marks, never recognised text. */
    const val ERASABLE = INK or AGENT
}

/**
 * Kotlin face of one page's native `SceneHandle`: the page model, undo stack and tools all live in Rust; this class
 * only moves bytes.
 *
 * Every tool method returns the produced **edit blob**: the edit is already applied to the local scene, recorded in the
 * open undo gesture and queued on the sync core, all in one native call under one lock; [commit] announces it to the
 * [EditSink]. Every native call holds [lock], the same lock the
 * sync core's wrapper uses, because a `SceneHandle` reads the `SyncCore` replica on `refresh` and a handle must
 * never be entered from two threads at once. Reads for rendering ([renderOrder], [itemFloats], [takeDirty]) use
 * the same lock; they are short.
 */
@Suppress("TooManyFunctions") // a facade: one method per native scene call
class PageScene(
    private val syncHandle: Long,
    private val lock: Any,
    val page: ByteArray,
    entropy: Long,
    private val sink: EditSink,
    private val clock: () -> Long = System::currentTimeMillis,
) : AutoCloseable {
    private var handle: Long = synchronized(lock) { Native.sceneOpen(syncHandle, page, entropy) }

    /** Called (on whichever thread changed the scene) after a change the screen must show. */
    @Volatile
    var onChanged: (() -> Unit)? = null

    private val listeners = CopyOnWriteArrayList<() -> Unit>()

    /**
     * Adds a listener told after every change of the scene's items, on the thread that made it (the pen's, the sync's,
     * the scheduler's). Unlike [onChanged] it is a list, so the page view can follow the scene alongside the renderer.
     */
    fun addListener(listener: () -> Unit) {
        listeners += listener
    }

    fun removeListener(listener: () -> Unit) {
        listeners -= listener
    }

    /** Called with a short reason when a tool's edit was refused (for example an op too large); nothing changed. */
    @Volatile
    var onError: ((String) -> Unit)? = null

    /** Called after the undo/redo availability may have changed. */
    @Volatile
    var onHistoryChanged: (() -> Unit)? = null

    // ---- remote changes ----------------------------------------------------------------------------------------

    /** Mirrors the changes of one `syncReceive` outcome blob that concern this page. */
    fun refresh(outcome: ByteArray) {
        locked { Native.sceneRefresh(it, syncHandle, outcome) }
        changed()
    }

    /** Re-reads the whole page from the replica (after a snapshot). */
    fun reload() {
        locked { Native.sceneReload(it, syncHandle) }
        changed()
    }

    // ---- gestures and history ----------------------------------------------------------------------------------

    /** Everything until [endGesture] is one undo step. */
    fun beginGesture() = locked { Native.sceneGesture(it, 1) }

    fun endGesture() {
        locked { Native.sceneGesture(it, 0) }
        onHistoryChanged?.invoke()
    }

    /** Bit 0: can undo, bit 1: can redo. */
    val history: Int get() = locked { Native.sceneHistory(it) }

    val canUndo: Boolean get() = history and 1 != 0

    val canRedo: Boolean get() = history and 2 != 0

    /** Undoes the newest still-valid step; true when something changed. */
    fun undo(): Boolean = commitChange(tool { Native.sceneUndo(it, syncHandle, clock()) })

    fun redo(): Boolean = commitChange(tool { Native.sceneRedo(it, syncHandle, clock()) })

    /** Deletes every item on the page (one undo step). */
    fun clearPage(): Boolean {
        beginGesture()
        try {
            val ids = renderOrder(Layers.ALL)
            return commitChange(deleteItems(ids))
        } finally {
            endGesture()
        }
    }

    // ---- tools (each returns an edit blob) ---------------------------------------------------------------------

    /** A fresh item id to stamp on the live deltas and on the committed stroke. */
    fun newItemId(): ByteArray = locked { Native.sceneNewItemId(it, clock()) }

    /** The finished pen stroke: [samples] interleaved x, y, pressure, tilt, azimuth, [times] µs since pen-down. */
    @Suppress("LongParameterList")
    fun addStroke(
        id: ByteArray,
        preset: Int,
        argb: Int,
        scale: Float,
        seed: Long,
        samples: FloatArray,
        times: IntArray,
    ): ByteArray =
        tool {
            Native.sceneAddStroke(it, id, intArrayOf(preset, argb), scale, seed, samples, times, syncHandle, clock())
        }

    /** A shape dragged from ([x0], [y0]) to ([x1], [y1]); see [ShapeStyle]. */
    fun addShape(
        id: ByteArray,
        x0: Float,
        y0: Float,
        x1: Float,
        y1: Float,
        style: ShapeStyle,
    ): ByteArray =
        tool {
            Native.sceneAddShape(
                it,
                id,
                floatArrayOf(x0, y0, x1, y1, style.kind.param),
                intArrayOf(
                    style.kind.code,
                    if (style.constrain) 1 else 0,
                    style.preset,
                    style.argb,
                    style.fillArgb,
                    if (style.hasFill) 1 else 0,
                ),
                style.scale,
                syncHandle,
                clock(),
            )
        }

    /** Drags handle [handleCode] (`Handle` order in core) of shape [id] to ([x], [y]). */
    fun resizeShape(
        id: ByteArray,
        handleCode: Int,
        x: Float,
        y: Float,
        constrain: Boolean = false,
    ): ByteArray = tool { Native.sceneResizeShape(it, id, handleCode, x, y, constrain, syncHandle, clock()) }

    /** One eraser chunk; [path] is interleaved x, y in page px. Call inside a gesture, once per chunk. */
    fun erase(
        path: FloatArray,
        radius: Float,
        precise: Boolean,
        layers: Int = Layers.INK,
    ): ByteArray = tool { Native.sceneErase(it, path, radius, precise, layers, syncHandle, clock()) }

    /** Ids inside a lasso polygon (interleaved x, y), bottom to top, as a concatenation of 16-byte ids. */
    fun lasso(
        polygon: FloatArray,
        layers: Int = Layers.INK,
    ): ByteArray = locked { Native.sceneLasso(it, polygon, layers) }

    fun hitTest(
        x: Float,
        y: Float,
        tolerance: Float,
        layers: Int = Layers.INK,
    ): ByteArray = locked { Native.sceneHitTest(it, x, y, tolerance, layers) }

    fun deleteItems(ids: ByteArray): ByteArray = tool { Native.sceneDeleteItems(it, ids, syncHandle, clock()) }

    fun moveItems(
        ids: ByteArray,
        dx: Float,
        dy: Float,
    ): ByteArray = tool { Native.sceneMoveItems(it, ids, dx, dy, syncHandle, clock()) }

    /** [layer]: 0 ink, 1 agent, 2 text. */
    fun setLayer(
        ids: ByteArray,
        layer: Int,
    ): ByteArray = tool { Native.sceneSetLayer(it, ids, layer, syncHandle, clock()) }

    // ---- frames (the design harness) ---------------------------------------------------------------------------

    /** The page's frame items as JSON: `[{"id","project","route","preset","cssW","cssH","rect":[x,y,w,h]}]`. */
    fun frames(): String = locked { Native.sceneFrames(it) }

    /** Adds a frame; [json] is `{project, route, preset, cssW, cssH, x, y}` with the position in page px. */
    fun addFrame(json: String): ByteArray = tool { Native.sceneAddFrame(it, json, syncHandle, clock()) }

    /** Switches a frame's size preset (`cssW`/`cssH` matter for `custom` only). */
    fun setFramePreset(
        id: ByteArray,
        preset: String,
        cssW: Int,
        cssH: Int,
    ): ByteArray = tool { Native.sceneSetFramePreset(it, id, preset, cssW, cssH, syncHandle, clock()) }

    /** Points a frame at another route of its project. */
    fun setFrameRoute(
        id: ByteArray,
        route: String,
    ): ByteArray = tool { Native.sceneSetFrameRoute(it, id, route, syncHandle, clock()) }

    /** A finished pen stroke on a frame, bound (or pinned) by [bindingJson] (K1); samples are in page px. */
    @Suppress("LongParameterList")
    fun addBoundStroke(
        id: ByteArray,
        preset: Int,
        argb: Int,
        scale: Float,
        seed: Long,
        samples: FloatArray,
        times: IntArray,
        bindingJson: String,
    ): ByteArray =
        tool {
            Native.sceneAddBoundStroke(
                it,
                id,
                intArrayOf(preset, argb),
                scale,
                seed,
                samples,
                times,
                bindingJson,
                syncHandle,
                clock(),
            )
        }

    /** Merges a frame's bridge layout (K1); true when the page's visible output changed and the canvas must redraw. */
    fun setFrameLayout(
        id: ByteArray,
        json: String,
    ): Boolean {
        val visible = locked { Native.sceneSetFrameLayout(it, id, json) }
        if (visible) changed()
        return visible
    }

    /** The element keys the frame's bound strokes need watched: a JSON array. */
    fun frameKeys(id: ByteArray): String = locked { Native.sceneFrameKeys(it, id) }

    /** Every mark of the page as JSON (K1). */
    fun marks(): String = locked { Native.sceneMarks(it) }

    /** Sets a mark's status as one undoable LWW edit; true when the mark changed. */
    fun setMarkStatus(
        id: ByteArray,
        status: String,
    ): Boolean = commitChange(tool { Native.sceneSetMarkStatus(it, id, status, syncHandle, clock()) })

    /** Shows or hides done marks (a local view flag); true when the page's visible output changed. */
    fun setShowDone(on: Boolean): Boolean {
        val visible = locked { Native.sceneSetShowDone(it, on) }
        if (visible) changed()
        return visible
    }

    // ---- commit ------------------------------------------------------------------------------------------------

    /**
     * Announces a tool's edit blob: the tool already applied it to the scene and queued it on the sync core in one
     * native call, so this only asks for a persist/push and a repaint. False when the blob held no edits.
     */
    fun commit(edits: ByteArray): Boolean = commitChange(edits)

    private fun commitChange(edits: ByteArray): Boolean {
        if (isEmpty(edits)) return false
        sink.committed()
        changed()
        onHistoryChanged?.invoke()
        return true
    }

    // ---- render data -------------------------------------------------------------------------------------------

    /** Live item ids on [layers] in paint order, concatenated 16-byte ids. */
    fun renderOrder(layers: Int = Layers.ALL): ByteArray = locked { Native.sceneRenderOrder(it, layers) }

    fun itemFloats(id: ByteArray): FloatArray = locked { Native.sceneItemFloats(it, id) }

    fun itemText(id: ByteArray): String = locked { Native.sceneItemText(it, id) }

    /** Ids whose render data changed since the last call; byte 0 is 1 when everything changed, then 16-byte ids. */
    fun takeDirty(): ByteArray = locked { Native.sceneTakeDirty(it) }

    /** Runs [block] holding the scene lock, so several reads see one consistent state. */
    fun <T> atomically(block: () -> T): T = synchronized(lock) { block() }

    override fun close() {
        val h = handle
        if (h == 0L) return
        handle = 0L
        synchronized(lock) { Native.sceneFree(h) }
    }

    private fun changed() {
        onChanged?.invoke()
        // Copy-on-write: a listener may add or remove one while the scene is being changed from its thread.
        for (listener in listeners) listener()
    }

    /**
     * A tool: applies and commits natively. A refused commit (an op too large to ship) leaves the scene and the
     * outbox as they were, is reported through [onError], and yields an empty blob, so the pen path never throws.
     * Native errors surface as RuntimeException; a closed scene (handle 0) is a caller bug and still throws.
     */
    @Suppress("TooGenericExceptionCaught")
    private fun tool(block: (Long) -> ByteArray): ByteArray =
        try {
            locked(block)
        } catch (error: RuntimeException) {
            if (handle == 0L) throw error
            onError?.invoke(error.message.orEmpty())
            changed()
            ByteArray(0)
        }

    private fun <T> locked(block: (Long) -> T): T =
        synchronized(lock) {
            check(handle != 0L) { "PageScene closed" }
            block(handle)
        }

    companion object {
        /** True when an edit blob carries zero edits (its `u32` count is zero). */
        fun isEmpty(edits: ByteArray): Boolean =
            edits.size < COUNT_BYTES || edits.take(COUNT_BYTES).all { it == 0.toByte() }

        private const val COUNT_BYTES = 4
    }
}
