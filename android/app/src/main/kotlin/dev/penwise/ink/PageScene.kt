package dev.penwise.ink

import dev.penwise.core.Native

/** Where the edits a [PageScene] tool produced go: the sync core stamps and queues them (and persists the outbox). */
fun interface EditSink {
    fun commit(
        page: ByteArray,
        edits: ByteArray,
        nowMs: Long,
    )
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
 * Every tool method returns the produced **edit blob** (already applied to the local scene and recorded in the
 * open undo gesture); [commit] hands a blob to the [EditSink]. Every native call holds [lock], the same lock the
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
    fun undo(): Boolean = commitChange(locked { Native.sceneUndo(it) })

    fun redo(): Boolean = commitChange(locked { Native.sceneRedo(it) })

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
    ): ByteArray = locked { Native.sceneAddStroke(it, id, intArrayOf(preset, argb), scale, seed, samples, times) }

    /** A shape dragged from ([x0], [y0]) to ([x1], [y1]); see [ShapeStyle]. */
    fun addShape(
        id: ByteArray,
        x0: Float,
        y0: Float,
        x1: Float,
        y1: Float,
        style: ShapeStyle,
    ): ByteArray =
        locked {
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
            )
        }

    /** Drags handle [handleCode] (`Handle` order in core) of shape [id] to ([x], [y]). */
    fun resizeShape(
        id: ByteArray,
        handleCode: Int,
        x: Float,
        y: Float,
        constrain: Boolean = false,
    ): ByteArray = locked { Native.sceneResizeShape(it, id, handleCode, x, y, constrain) }

    /** One eraser chunk; [path] is interleaved x, y in page px. Call inside a gesture, once per chunk. */
    fun erase(
        path: FloatArray,
        radius: Float,
        precise: Boolean,
        layers: Int = Layers.INK,
    ): ByteArray = locked { Native.sceneErase(it, path, radius, precise, layers) }

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

    fun deleteItems(ids: ByteArray): ByteArray = locked { Native.sceneDeleteItems(it, ids) }

    fun moveItems(
        ids: ByteArray,
        dx: Float,
        dy: Float,
    ): ByteArray = locked { Native.sceneMoveItems(it, ids, dx, dy) }

    /** [layer]: 0 ink, 1 agent, 2 text. */
    fun setLayer(
        ids: ByteArray,
        layer: Int,
    ): ByteArray = locked { Native.sceneSetLayer(it, ids, layer) }

    // ---- commit ------------------------------------------------------------------------------------------------

    /** Hands a tool's edit blob to the sync core and requests a repaint; false when the blob held no edits. */
    fun commit(edits: ByteArray): Boolean = commitChange(edits)

    private fun commitChange(edits: ByteArray): Boolean {
        if (isEmpty(edits)) return false
        sink.commit(page, edits, clock())
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
