package dev.penwise.ink

import android.app.Application
import android.util.Log
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import dev.penwise.BuildConfig
import dev.penwise.R
import dev.penwise.frames.AgentPage
import dev.penwise.frames.DesignProject
import dev.penwise.frames.FrameAgent
import dev.penwise.frames.FrameControl
import dev.penwise.frames.FrameSpec
import dev.penwise.frames.MoveTarget
import dev.penwise.frames.ProjectsClient
import dev.penwise.frames.SceneMark
import dev.penwise.overview.OverviewController
import dev.penwise.sync.Ids
import dev.penwise.sync.Kind
import dev.penwise.sync.Outcome
import dev.penwise.sync.SyncEngine
import dev.penwise.sync.SyncListener
import dev.penwise.sync.SyncStatus
import dev.penwise.tools.ToolState
import kotlinx.coroutines.Dispatchers
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
import java.io.IOException
import java.security.SecureRandom
import java.util.concurrent.Executors

private const val TAG = "PenwiseSync"
private const val RETIRE_DELAY_MS = 1_000L

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

    /** Frames, marks and watched keys of the open page (the design harness); the core owns their truth. */
    internal val frameSession =
        FrameSession(
            viewModelScope,
            catalogDispatcher,
            { _scene.value },
            { action -> edit(action = action) },
            { engine?.core?.outbox()?.pending ?: 0 },
        )

    /** Reaches the frame layer from outside the composition (agent camera moves). */
    internal val frameControl = FrameControl()

    /** Camera moves other devices ask for (`SwitchPage`, `Focus`), held back while the owner draws. */
    internal val agent =
        FrameAgent(
            viewModelScope,
            viewport,
            pen,
            frameControl,
            AgentPage(
                current = { shown },
                open = ::selectPage,
                title = { id ->
                    _pages.value
                        .page(id)
                        ?.title
                        .orEmpty()
                },
                specs = { frameSession.specs.value },
                contentBounds = {
                    _scene.value?.contentBounds()?.let { PageRect(it[0], it[1], it[2] - it[0], it[3] - it[1]) }
                },
                openOverview = ::openOverview,
            ),
        )

    init {
        // A camera move that waited for a page's frames can go on once a sweep of them has finished.
        viewModelScope.launch { frameSession.sweeps.collect { agent.framesChanged() } }
    }

    /** The open page's frames, ready for the frame layer (each URL on the host sync uses now). */
    val frames: StateFlow<List<FrameSpec>> = frameSession.specs

    /** The projects the server has linked, for the new-frame picker; cached, refreshed on reconnect. */
    val projects: StateFlow<List<DesignProject>> = frameSession.projects

    /** The open page's marks (hidden ones have no rect), for the chip overlay. */
    val marks: StateFlow<List<SceneMark>> = frameSession.marks

    /** The element keys each frame's bound strokes need watched (`pw:watch`). */
    val watchKeys: StateFlow<Map<String, List<String>>> = frameSession.watchKeys

    /** Whether done marks are shown (the core hides them by default). */
    val showDone: StateFlow<Boolean> = frameSession.showDone

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
        remoteMove(outcome)
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

    /** Another device switched page or asked for a frame to be fit: the camera follows unless the owner is drawing. */
    private fun remoteMove(outcome: Outcome) {
        val page = outcome.page?.let(Ids::hex) ?: return
        if (outcome.kind != Kind.SWITCH_PAGE && outcome.kind != Kind.FOCUS) return
        val target = MoveTarget(page, outcome.focusItem?.let(Ids::hex), label = "", fit = outcome.kind == Kind.FOCUS)
        viewModelScope.launch { agent.offer(target) }
    }

    /** The overview's state; [openOverview] and the overview button open it, the screen shows it. */
    internal val overview =
        OverviewController().also {
            it.attachSketches(viewModelScope, { frameSession.server.value }) { engine?.core?.outbox()?.pending ?: 0 }
        }

    /** A pinch out past the locked frame's fit scale, the overview button, or an agent's `Focus` without an item. */
    fun openOverview() = overview.request(shown, frameSession.specs.value, frameControl.layer?.lockedFrame)

    /** The owner double-tapped a frame: other devices may fit it too (only a server that speaks protocol 2). */
    fun frameFocused(frame: String) {
        val open = engine ?: return
        val page = Ids.bytes(shown) ?: return
        val item = Ids.bytes(frame) ?: return
        viewModelScope.launch(catalogDispatcher) {
            if (open.core.supportsFocus()) open.core.focus(page, item)?.let(open::send)
        }
    }

    private inner class Listener : SyncListener {
        override fun onOutcome(outcome: Outcome) = this@PageViewModel.onOutcome(outcome)

        override fun onStatus(status: SyncStatus) {
            _status.value = status
            frameSession.updateEndpoint(syncHttpBase(), settings.token)
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
            frameSession.updateEndpoint(syncHttpBase(), settings.token)
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
            frameSession.clear()
            next.addListener { frameSession.schedule(next) }
            val old = _scene.value
            _scene.value = next
            // The camera is free again until the new page's frames lock it.
            viewport.unlock()
            viewport.reset()
            shown = id
            withContext(Dispatchers.Default) {
                runCatching(next::reload)
                // The show-done flag is the owner's view preference, not a scene default.
                frameSession.reapplyShowDone(next)
            }
            frameSession.schedule(next)
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
            frameSession.updateProjects(list)
        }
    }

    /** Adds a frame of [project] at [route]; one synced op, undoable like any other item. */
    @Suppress("LongParameterList")
    fun addFrame(
        project: String,
        route: String,
        preset: String,
        cssW: Int,
        cssH: Int,
        x: Float,
        y: Float,
    ) = frameSession.addFrame(project, route, preset, cssW, cssH, x, y)

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
