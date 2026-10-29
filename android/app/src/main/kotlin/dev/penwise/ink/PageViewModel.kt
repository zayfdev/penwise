package dev.penwise.ink

import android.app.Application
import android.util.Log
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import dev.penwise.BuildConfig
import dev.penwise.R
import dev.penwise.frames.DesignProject
import dev.penwise.frames.FramePreset
import dev.penwise.frames.FrameProjection
import dev.penwise.frames.FrameRoutes
import dev.penwise.frames.FrameSpec
import dev.penwise.frames.ProjectsClient
import dev.penwise.frames.SceneFrames
import dev.penwise.sync.Ids
import dev.penwise.sync.Kind
import dev.penwise.sync.Outcome
import dev.penwise.sync.SyncEngine
import dev.penwise.sync.SyncListener
import dev.penwise.sync.SyncStatus
import dev.penwise.tools.ToolState
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.asCoroutineDispatcher
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.flow.updateAndGet
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import org.json.JSONObject
import java.io.IOException
import java.security.SecureRandom
import java.util.concurrent.Executors

private const val TAG = "PenwiseSync"
private const val RETIRE_DELAY_MS = 1_000L

/** A change of the scene can arrive in bursts (a frame dragged, a preset switched); one frame read per burst. */
private const val FRAMES_DEBOUNCE_MS = 60L

/**
 * Page and tool state that survives configuration changes; the thin shell around [SyncEngine].
 *
 * The tablet is offline-first: the page model, undo, erase and every storage rule live in Rust, and the replica
 * persists itself, so drawing never waits on the server. This class opens a [PageScene] for the page on screen,
 * routes server frames to it, and exposes the notebook/page catalog the UI lists.
 */
class PageViewModel(
    application: Application,
) : AndroidViewModel(application) {
    val tools = ToolState()

    private val settings = SyncSettings(application)
    private val random = SecureRandom()

    /** Pictures placed on pages, fetched by asset hash and cached. */
    val images =
        PageImages { asset, maxPixels ->
            // The connected endpoint first (away from home only Tailscale answers), then the others.
            connectedFirst(engine?.connectedUrl, settings.endpoints())
                .firstNotNullOfOrNull { LibraryClient(it, settings.token).asset(asset, maxPixels) }
        }

    /** Whether the pen is writing or hovering: Back is ignored, and a left page is kept open, while it is. */
    val pen = PenActivity()

    private val _pages = MutableStateFlow(PageList.EMPTY)

    /** Every live page in reading order, grouped by notebook; empty until [loadPage] finishes. */
    val pages: StateFlow<PageList> = _pages.asStateFlow()

    private val _scene = MutableStateFlow<PageScene?>(null)

    /** The scene of the page on screen; `null` until the library has loaded. */
    val scene: StateFlow<PageScene?> = _scene.asStateFlow()

    private val frameProjection = FrameProjection()
    private var framesJob: Job? = null

    /** The open page's frames, ready for the frame layer (each URL on the host sync uses now). */
    val frames: StateFlow<List<FrameSpec>> = frameProjection.specs

    /** The projects the server has linked, for the new-frame picker; cached, refreshed on reconnect. */
    val projects: StateFlow<List<DesignProject>> = frameProjection.listed

    /** Pan and zoom of the page on screen; refitted whenever the page changes. */
    val viewport = ViewportState()

    private val _status = MutableStateFlow(SyncStatus.STOPPED)

    /** State of the link to the PC server. */
    val status: StateFlow<SyncStatus> = _status.asStateFlow()

    private val startup = LibraryStartup(viewModelScope, onImported = { publishCatalog(null) })
    private val pageLock = Mutex()
    private val publishLock = Any()
    private val retiring = ArrayList<PageScene>()
    private var started = false

    /** Catalog changes and status reads run here, one at a time and in order, never on the UI thread. */
    private val catalogThread =
        Executors.newSingleThreadExecutor { Thread(it, "penwise-catalog").apply { isDaemon = true } }
    private val catalogDispatcher = catalogThread.asCoroutineDispatcher()

    /** Notebook and page create/rename/delete; each runs on the catalog thread, never the UI thread. */
    val catalog =
        CatalogCommands(
            viewModelScope,
            catalogDispatcher,
            { engine?.library },
            { _pages.value },
            ::publishCatalog,
            { syncUi.notice.value = it },
        )

    /** Sync health and the dialogs' data (state file, kept-aside files, server identity, dead letters). */
    val syncUi = SyncUiState(catalogDispatcher) { engine }

    @Volatile private var ready = false

    @Volatile private var shown = ""

    /** The page the server said was current at the last `Welcome`; only used when nothing was open before. */
    @Volatile
    private var serverPage: String? = null
    private var pendingFixture: String? = null

    @Volatile
    private var engine: SyncEngine? = null

    /** Streams the in-progress stroke to the PC mirror; `null` until the engine is open (before any scene). */
    @Volatile
    var liveOut: LiveOut? = null
        private set

    /** URL and token shown in the settings dialog (the token is empty when the server has none). */
    val syncUrl: String get() = settings.displayedUrl()
    val syncToken: String get() = settings.token

    /** The user's answer to "the server is a different one": keep the ink (re-send it) or drop it. */
    fun resolveServerChange(keep: Boolean) = syncUi.resolveServerChange(viewModelScope, keep, ::onOutcome)

    private fun onOutcome(outcome: Outcome) {
        if (outcome.kind == Kind.WELCOME) serverPage = outcome.page?.let(Ids::hex)
        // The core may have been answered since this frame was queued: only a still-held core asks the question.
        if (outcome.serverChanged && engine?.core?.serverChangePending() != false) syncUi.serverChanged.value = true
        if (!ready) return
        if (outcome.catalogChanged) publishCatalog(null)
        if (outcome.hasChanges) {
            try {
                // A resync blanked the replica: re-read the whole page rather than trusting the change list.
                _scene.value?.let { if (outcome.reconnect) it.reload() else it.refresh(outcome.raw) }
            } catch (_: IllegalStateException) {
                Log.i(TAG, "scene closed while a frame arrived") // the page was just left.
            }
        }
    }

    private inner class Listener : SyncListener {
        override fun onOutcome(outcome: Outcome) = this@PageViewModel.onOutcome(outcome)

        override fun onStatus(status: SyncStatus) {
            _status.value = status
            frameProjection.updateEndpoint(syncHttpBase(), settings.token)
            if (status == SyncStatus.ONLINE) refreshProjects()
            val app = getApplication<Application>()
            val online = status == SyncStatus.ONLINE
            val where = engine?.connectedUrl ?: settings.displayedUrl()
            SyncService.start(app, if (online) app.getString(R.string.sync_connected, where) else "", online)
        }

        override fun onCaughtUp() = startup.caughtUp()

        override fun onUnreachable() = startup.unreachable()

        override fun onStateFailing(failing: Boolean) {
            syncUi.stateFailing.value = failing
        }
    }

    /**
     * Opens the library and the active page, then connects. Opening reads and decodes the whole saved replica, so
     * it runs off the main thread. First launch after the upgrade imports the old page files once, after the
     * replica is caught up with the server (see [LegacyImport.runWhenCaughtUp]).
     */
    fun loadPage() {
        if (started) return
        started = true
        viewModelScope.launch {
            val opened = openEngine() ?: return@launch
            engine = opened
            frameProjection.updateEndpoint(syncHttpBase(), settings.token)
            syncUi.recovered.value = opened.recovered.map { it.name }
            liveOut = SyncLiveOut(opened.core, opened::send)
            opened.start()
            withContext(Dispatchers.IO) { startup.prepare(opened) }
            ready = true
            val list = withContext(Dispatchers.IO) { opened.library.list(settings.activePage ?: serverPage) }
            _pages.value = list
            showActive()
            pendingFixture?.let { runReplay(it) }
            pendingFixture = null
        }
    }

    // Native restore errors are RuntimeExceptions, a state file that cannot be kept aside is an IOException.
    @Suppress("TooGenericExceptionCaught")
    private suspend fun openEngine(): SyncEngine? =
        withContext(Dispatchers.IO) {
            try {
                SyncEngine(
                    filesDir = getApplication<Application>().filesDir,
                    endpoints = settings::endpoints,
                    token = { settings.token },
                    listener = Listener(),
                )
            } catch (error: Exception) {
                Log.e(TAG, "sync state unavailable: ${error.message}")
                syncUi.startupError.value = error.message.orEmpty().ifBlank { "storage error" }
                null
            }
        }

    /** Re-reads the catalog (off the UI thread); moves to another page when the open one is gone. */
    private fun publishCatalog(active: String?) {
        val open = engine ?: return
        // Read and publish under one lock so catalog snapshots land in the order they were read, and keep the page
        // the user picked meanwhile (selectPage can run between the read and the write).
        val list =
            synchronized(publishLock) {
                val fresh = open.library.list(active)
                _pages.updateAndGet { current -> fresh.withPreferred(active ?: current.active.ifEmpty { null }) }
            }
        if (list.active != shown) viewModelScope.launch { showActive() }
    }

    /**
     * Opens the page the catalog marks active, if it is not open already. Serialized: only the last switch loads.
     *
     * A frame can be applied between the moment the new scene reads the replica and the moment it is published;
     * that frame is refreshed into the *old* scene. So the new scene re-reads its page right after publishing:
     * from then on every frame reaches it, and nothing from before is missing.
     */
    private suspend fun showActive() {
        pageLock.withLock {
            val id = _pages.value.active
            val bytes = Ids.bytes(id)
            val open = engine
            if (bytes == null || id == shown || open == null) return
            val core = open.core
            val next =
                try {
                    withContext(Dispatchers.Default) {
                        core.whileOpen { handle -> PageScene(handle, core.lock, bytes, random.nextLong(), core) }
                    }
                } catch (_: IllegalStateException) {
                    return // the engine closed while the page was opening
                }
            next.onError = { syncUi.editRefused.value = it }
            // A frame of the page being left must not stay under the ink while the new page loads.
            frameProjection.updateItems(emptyList())
            next.addListener { scheduleFrames(next) }
            val old = _scene.value
            _scene.value = next
            viewport.reset()
            shown = id
            withContext(Dispatchers.Default) { runCatching(next::reload) }
            scheduleFrames(next)
            settings.activePage = id
            withContext(Dispatchers.Default) { open.showing(id) }
            old?.let { retire(it) }
        }
    }

    /** Closes a left page's scene a moment later: the renderer may still draw its last frame, the pen may hold it. */
    private fun retire(scene: PageScene) {
        retiring.add(scene)
        viewModelScope.launch {
            delay(RETIRE_DELAY_MS)
            while (pen.drawing) delay(RETIRE_DELAY_MS)
            if (retiring.remove(scene)) scene.close()
        }
    }

    /** Shows `id`. */
    fun selectPage(id: String) {
        if (id == _pages.value.active || _pages.value.page(id) == null) return
        _pages.update { if (it.page(id) != null) it.copy(active = id) else it }
        viewModelScope.launch { showActive() }
    }

    // ---- frames (the design harness) --------------------------------------------------------------------------

    /** Fetches the linked projects again; the layer caches them and this runs on every reconnect (C2). */
    fun refreshProjects() {
        val base = syncHttpBase() ?: return
        val token = settings.token
        viewModelScope.launch {
            val list = withContext(Dispatchers.IO) { ProjectsClient(base, token).list() } ?: return@launch
            frameProjection.updateProjects(list)
        }
    }

    /**
     * Adds a frame of [project] at [route], at [preset]'s size, its top-left at ([x], [y]) in page px: one synced op,
     * undoable and movable like any other item. The frame layer opens it from the scene's next frame read.
     */
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
        val item = frameProjection.item(id) ?: return
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
        if (frameProjection.item(id)?.route == path) return
        val bytes = Ids.bytes(id) ?: return
        edit { it.setFrameRoute(bytes, path) }
    }

    /** Re-reads the open page's frames after a scene change; a burst of changes becomes one read. */
    private fun scheduleFrames(scene: PageScene) {
        framesJob?.cancel()
        framesJob =
            viewModelScope.launch {
                delay(FRAMES_DEBOUNCE_MS)
                val json =
                    withContext(catalogDispatcher) {
                        try {
                            scene.frames()
                        } catch (_: IllegalStateException) {
                            null // the page was just left
                        }
                    } ?: return@launch
                if (_scene.value !== scene) return@launch
                frameProjection.updateItems(SceneFrames.parse(json))
            }
    }

    /** The HTTP base of the server the tablet reaches: the connected endpoint, or the first candidate while offline. */
    private fun syncHttpBase(): String? {
        val endpoint = engine?.connectedUrl ?: settings.endpoints().firstOrNull() ?: return null
        return LibraryClient.httpBase(endpoint).takeIf { it.isNotBlank() }
    }

    /** Undo on the page on screen. History work takes the core lock, so it runs off the UI thread, in order. */
    fun undo() = edit { it.undo() }

    fun redo() = edit { it.redo() }

    /** Clears [scene] (the page the confirm dialog was opened for, which may no longer be the one on screen). */
    fun clearPage(scene: PageScene) = edit(scene) { it.clearPage() }

    private fun edit(
        target: PageScene? = _scene.value,
        action: (PageScene) -> Unit,
    ) {
        // The selection (and its frame) describe the page as it was before this change.
        tools.selection.clear()
        val scene = target ?: return
        viewModelScope.launch(catalogDispatcher) {
            try {
                action(scene)
            } catch (_: IllegalStateException) {
                Log.i(TAG, "page closed before the edit ran") // the page was just left.
            }
        }
    }

    /**
     * Saves a new address/token and reconnects.
     *
     * A six-character code in the token field is a pairing code: it is exchanged for the real token (and the
     * server's URL) first, so nobody has to copy 43 characters by hand.
     */
    fun updateSync(
        url: String?,
        token: String,
    ) {
        viewModelScope.launch {
            val update = withContext(Dispatchers.IO) { syncUpdateFor(url, token, ::pair) }
            if (update == null) {
                // Nothing changes: the code is not a token, and the old address and token may still work.
                syncUi.notice.value = R.string.sync_pair_failed
                return@launch
            }
            update.url?.let { settings.overrideUrl = it }
            settings.token = update.token
            engine?.start(forgetGood = true)
        }
    }

    /** Exchanges a pairing code at [url], or at each known endpoint when none was given. */
    private fun pair(
        url: String?,
        code: String,
    ): Pair<String, String>? {
        val bases = url?.takeIf { it.isNotBlank() }?.let(::listOf) ?: settings.endpoints()
        return bases.firstNotNullOfOrNull { runCatching { LibraryClient(it, "").pair(code) }.getOrNull() }
    }

    /**
     * Debug builds only: replays a recorded page onto the visible one, through the same scene → sync path a pen
     * stroke takes. Deferred until [loadPage] has opened a page.
     */
    fun requestReplay(path: String) {
        if (!BuildConfig.DEBUG) return
        if (ready) viewModelScope.launch { runReplay(path) } else pendingFixture = path
    }

    private suspend fun runReplay(path: String) {
        val open = _scene.value ?: return
        withContext(Dispatchers.Default) { FixtureReplay.replayOnto(getApplication(), path, open) }
    }

    /** Writes the sync state file now (the app is going to the background and may be killed). */
    fun flushState() {
        viewModelScope.launch(Dispatchers.IO) { engine?.flush() }
    }

    override fun onCleared() {
        val open = engine
        _scene.value?.close()
        retiring.forEach(PageScene::close)
        open?.close()
        catalogThread.shutdown()
        super.onCleared()
    }
}
