package dev.penwise.ink

import android.util.Log
import dev.penwise.R
import dev.penwise.frames.DesignProject
import dev.penwise.frames.FrameLayouts
import dev.penwise.frames.FrameMarks
import dev.penwise.frames.FramePreset
import dev.penwise.frames.FrameProjection
import dev.penwise.frames.FrameRoutes
import dev.penwise.frames.FrameServer
import dev.penwise.frames.FrameSpec
import dev.penwise.frames.LayoutKind
import dev.penwise.frames.OutboxWait
import dev.penwise.frames.RoundsClient
import dev.penwise.frames.SceneFrame
import dev.penwise.frames.SceneFrames
import dev.penwise.frames.SceneMark
import dev.penwise.frames.SendResult
import dev.penwise.frames.WindowUpdate
import dev.penwise.sync.Ids
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asSharedFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.json.JSONObject

/** A scene change can arrive in bursts (a frame dragged, a preset switched, a scroll frame); one read per burst. */
private const val FRAMES_DEBOUNCE_MS = 60L
private const val TAG = "PenwiseFrames"
private const val WINDOW_BUFFER = 16

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
    /** Ops of the local outbox still waiting for the server's `Ack`: Send waits for them (at most 3 s). */
    private val outbox: () -> Int = { 0 },
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

    private val _server = MutableStateFlow<FrameServer?>(null)

    /** The PC server's HTTP base and token (stills, hits, rounds); null while the tablet knows none. */
    val server: StateFlow<FrameServer?> = _server.asStateFlow()

    private val _windows = MutableSharedFlow<WindowUpdate>(extraBufferCapacity = WINDOW_BUFFER)

    /** The live window the core computed after each live layout; the frame layer places its WebView with it. */
    val windows: SharedFlow<WindowUpdate> = _windows.asSharedFlow()

    private val _notices = MutableSharedFlow<Int>(extraBufferCapacity = 1)

    /** Short messages for a toast (string resource ids). */
    val notices: SharedFlow<Int> = _notices.asSharedFlow()

    private val _showDone = MutableStateFlow(false)

    /** Whether done marks are shown (the core hides them by default). */
    val showDone: StateFlow<Boolean> = _showDone.asStateFlow()

    fun item(id: String): SceneFrame? = projection.item(id)

    fun updateProjects(next: List<DesignProject>) = projection.updateProjects(next)

    fun updateEndpoint(
        base: String?,
        token: String,
    ) {
        projection.updateEndpoint(base, token)
        _server.value = base?.let { FrameServer(it, token) }
    }

    private val _sweeps = MutableStateFlow(0)

    /** Counts finished sweeps of the open page: a frame list that was waited for may be there now. */
    val sweeps: StateFlow<Int> = _sweeps.asStateFlow()

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
                _sweeps.value += 1
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
     * Feeds one layout into the core (K1). The core repaints through the scene change when the visible output moved;
     * the layout itself is local and never synced. A live report also asks the core where the window sits now; a
     * demoted frame's still layout replaces the live one instead of merging into it.
     */
    fun setLayout(
        id: String,
        json: String,
        kind: LayoutKind,
    ) {
        val scene = current() ?: return
        val bytes = Ids.bytes(id) ?: return
        scope.launch(io) {
            try {
                // A still (new, or taking over from the live page) replaces the layout in one native step; only the
                // live page's reports merge into it.
                if (kind.replaces) scene.replaceFrameLayout(bytes, json) else scene.setFrameLayout(bytes, json)
                // The live page's window moves with the scroll it just reported; the core says where it sits now.
                if (kind == LayoutKind.LIVE) windowAfter(scene, bytes, id, json)
            } catch (_: IllegalStateException) {
                Log.i(TAG, "page closed before the layout arrived")
            }
        }
    }

    private fun windowAfter(
        scene: PageScene,
        bytes: ByteArray,
        id: String,
        json: String,
    ) {
        val window = scene.frameLiveWindow(bytes) ?: return
        val scroll = JSONObject(json).optJSONArray("scroll")
        _windows.tryEmit(
            WindowUpdate(id, window, scroll?.optDouble(0)?.toFloat() ?: 0f, scroll?.optDouble(1)?.toFloat() ?: 0f),
        )
    }

    private var focusedFrame: String? = null

    /** The live frame changed (null: none is live); the Send button sends the round of its project. */
    fun focused(frame: String?) {
        focusedFrame = frame
    }

    /**
     * Sends the open round of the focused frame's project, else of the page's first frame (spec §4), once the local
     * outbox is acknowledged (at most 3 s): a `409` means nothing new was drawn, which is only a toast.
     */
    fun sendRound() {
        val server = _server.value ?: return
        val project = focusedFrame?.let { projection.item(it)?.project } ?: specs.value.firstOrNull()?.project ?: return
        scope.launch(Dispatchers.IO) {
            // The last stroke must be on the server before the round closes, or it lands in the next one.
            OutboxWait.untilAcked(outbox)
            val message =
                when (RoundsClient.send(server, project)) {
                    SendResult.SENT -> R.string.round_sent
                    SendResult.NOTHING_TO_SEND -> R.string.round_nothing_to_send
                    SendResult.FAILED -> R.string.round_send_failed
                }
            _notices.tryEmit(message)
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
