package dev.penwise.ink

import android.util.Log
import dev.penwise.frames.DesignProject
import dev.penwise.frames.FrameLayouts
import dev.penwise.frames.FrameMarks
import dev.penwise.frames.FramePreset
import dev.penwise.frames.FrameProjection
import dev.penwise.frames.FrameRoutes
import dev.penwise.frames.FrameSpec
import dev.penwise.frames.SceneFrame
import dev.penwise.frames.SceneFrames
import dev.penwise.frames.SceneMark
import dev.penwise.sync.Ids
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.json.JSONObject

/** A scene change can arrive in bursts (a frame dragged, a preset switched, a scroll frame); one read per burst. */
private const val FRAMES_DEBOUNCE_MS = 60L
private const val TAG = "PenwiseFrames"

/**
 * The design harness' view-model side (M1/M2): the open page's frames, ready for the frame layer, plus the marks and
 * watched keys the core reports after each scene change. The core owns placement, visibility and grouping; this class
 * only feeds it layouts and projects what it says for the chips.
 *
 * [current] and [edit] keep it decoupled from the page's owning view model: the first reads the scene on screen (null
 * between pages), the second runs a tool edit on the scene thread.
 */
@Suppress("TooManyFunctions") // one entry per harness action, kept flat on purpose
internal class FrameSession(
    private val scope: CoroutineScope,
    private val io: CoroutineDispatcher,
    private val current: () -> PageScene?,
    private val edit: ((PageScene) -> Unit) -> Unit,
) {
    private val projection = FrameProjection()
    private var job: Job? = null

    /** The open page's frames, ready for the frame layer (each URL on the host sync uses now). */
    val specs: StateFlow<List<FrameSpec>> = projection.specs

    /** The projects the server has linked, for the new-frame picker; cached, refreshed on reconnect (C2). */
    val projects: StateFlow<List<DesignProject>> = projection.listed

    private val _marks = MutableStateFlow<List<SceneMark>>(emptyList())

    /** The open page's marks (hidden ones have no rect), for the chip overlay. */
    val marks: StateFlow<List<SceneMark>> = _marks.asStateFlow()

    private val _watchKeys = MutableStateFlow<Map<String, List<String>>>(emptyMap())

    /** The element keys each frame's bound strokes need watched (`pw:watch`). */
    val watchKeys: StateFlow<Map<String, List<String>>> = _watchKeys.asStateFlow()

    private val _showDone = MutableStateFlow(false)

    /** Whether done marks are shown (the core hides them by default). */
    val showDone: StateFlow<Boolean> = _showDone.asStateFlow()

    fun item(id: String): SceneFrame? = projection.item(id)

    fun updateProjects(next: List<DesignProject>) = projection.updateProjects(next)

    fun updateEndpoint(
        base: String?,
        token: String,
    ) = projection.updateEndpoint(base, token)

    /** The page was left: its frames, marks and watch list must not stay under the ink. */
    fun clear() {
        projection.updateItems(emptyList())
        _marks.value = emptyList()
        _watchKeys.value = emptyMap()
    }

    /** Re-reads the page after a scene change; one read per burst, off the UI thread. */
    fun schedule(scene: PageScene) {
        job?.cancel()
        job =
            scope.launch {
                delay(FRAMES_DEBOUNCE_MS)
                val sweep =
                    withContext(io) {
                        try {
                            Sweep.of(scene)
                        } catch (_: IllegalStateException) {
                            null // the page was just left
                        }
                    } ?: return@launch
                if (current() !== scene) return@launch
                projection.updateItems(sweep.frames)
                _marks.value = FrameMarks.parse(sweep.marks)
                _watchKeys.value = sweep.watch
            }
    }

    /** A new page: its scene starts with done marks hidden, so the owner's preference is applied again. */
    fun reapplyShowDone(scene: PageScene) {
        if (_showDone.value) runCatching { scene.setShowDone(true) }
    }

    /**
     * Adds a frame of [project] at [route], at [preset]'s size, its top-left at ([x], [y]) in page px: one synced op,
     * undoable and movable like any other item. The frame layer opens it from the scene's next frame read.
     */
    @Suppress("LongParameterList")
    fun addFrame(
        project: String,
        route: String,
        preset: String,
        cssW: Int,
        cssH: Int,
        x: Float,
        y: Float,
    ) {
        val path = FrameRoutes.normalize(route) ?: return
        val size = FramePreset.byKey(preset)
        val width = size?.cssWidth ?: cssW
        val height = size?.cssHeight ?: cssH
        if (project.isEmpty() || width <= 0 || height <= 0) return
        val json =
            JSONObject()
                .put("project", project)
                .put("route", path)
                .put("preset", size?.key ?: FramePreset.CUSTOM)
                .put("cssW", width)
                .put("cssH", height)
                .put("x", x.toDouble())
                .put("y", y.toDouble())
                .toString()
        edit { it.addFrame(json) }
    }

    /** Switches a frame's preset: its page reflows at the new width (one op, no reload). */
    fun setFramePreset(
        id: String,
        preset: String,
    ) {
        val target = FramePreset.byKey(preset) ?: return
        val item = projection.item(id) ?: return
        if (item.preset == target.key && item.cssWidth == target.cssWidth && item.cssHeight == target.cssHeight) return
        val bytes = Ids.bytes(id) ?: return
        edit { it.setFramePreset(bytes, target.key, target.cssWidth, target.cssHeight) }
    }

    /**
     * Points a frame at another route of its project. A link clicked inside the page reports its route through the
     * same call: the page is already there, so the layer adopts the item without loading anything again.
     */
    fun setFrameRoute(
        id: String,
        route: String,
    ) {
        val path = FrameRoutes.normalize(route) ?: return
        if (projection.item(id)?.route == path) return
        val bytes = Ids.bytes(id) ?: return
        edit { it.setFrameRoute(bytes, path) }
    }

    /**
     * The owner's pick among a variant group (§4 variants, P2): one undoable edit that un-chooses the rest. The chips
     * follow from the core's next sweep; nothing is sent to the server here.
     */
    fun setFrameChosen(
        id: String,
        chosen: Boolean,
    ) {
        val item = projection.item(id) ?: return
        if (item.chosen == chosen) return
        val bytes = Ids.bytes(id) ?: return
        edit { it.setFrameChosen(bytes, chosen) }
    }

    /**
     * Feeds one `pw:layout` message into the core (K1). The core repaints through the scene change when the visible
     * output moved; the layout itself is local and never synced.
     */
    fun setLayout(
        id: String,
        json: String,
    ) {
        val scene = current() ?: return
        val bytes = Ids.bytes(id) ?: return
        scope.launch(io) {
            try {
                scene.setFrameLayout(bytes, json)
            } catch (_: IllegalStateException) {
                Log.i(TAG, "page closed before the layout arrived")
            }
        }
    }

    /** Sets a mark's status; the core records one undoable edit and the next sweep refreshes the chips. */
    fun setMarkStatus(
        markId: String,
        status: String,
    ) {
        val bytes = Ids.bytes(markId) ?: return
        edit { it.setMarkStatus(bytes, status) }
    }

    /** Shows or hides done marks; a local view flag, applied to every page the owner opens. */
    fun setShowDone(on: Boolean) {
        if (_showDone.value == on) return
        _showDone.value = on
        val scene = current() ?: return
        scope.launch(io) {
            try {
                scene.setShowDone(on)
            } catch (_: IllegalStateException) {
                Log.i(TAG, "page closed before show-done changed")
            }
        }
    }

    /** Frames, marks and watch keys, read under one scene lock. */
    private class Sweep(
        val frames: List<SceneFrame>,
        val marks: String,
        val watch: Map<String, List<String>>,
    ) {
        companion object {
            fun of(scene: PageScene): Sweep {
                val frames = SceneFrames.parse(scene.frames())
                val watch = LinkedHashMap<String, List<String>>()
                for (frame in frames) {
                    val id = Ids.bytes(frame.id) ?: continue
                    watch[frame.id] = FrameLayouts.keys(scene.frameKeys(id))
                }
                return Sweep(frames, scene.marks(), watch)
            }
        }
    }
}
