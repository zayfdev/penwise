package dev.penwise.ink

import android.app.Application
import android.util.Log
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import dev.penwise.BuildConfig
import dev.penwise.R
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
        PageImages { asset ->
            LibraryClient(settings.endpoints().firstOrNull().orEmpty(), settings.token).asset(asset)
        }

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
    private val retiring = ArrayList<PageScene>()
    private var started = false

    /** Catalog changes and status reads run here, one at a time and in order, never on the UI thread. */
    private val catalogThread =
        Executors.newSingleThreadExecutor { Thread(it, "penwise-catalog").apply { isDaemon = true } }
    private val catalogDispatcher = catalogThread.asCoroutineDispatcher()

    /** Notebook and page create/rename/delete; each runs on the catalog thread, never the UI thread. */
    val catalog =
        CatalogCommands(viewModelScope, catalogDispatcher, { engine?.library }, { _pages.value }, ::publishCatalog)

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
        if (outcome.serverChanged) syncUi.serverChanged.value = true
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
            if (status == SyncStatus.ONLINE) {
                val app = getApplication<Application>()
                SyncService.start(app, app.getString(R.string.sync_connected, settings.displayedUrl()))
            }
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
        val list = open.library.list(active ?: _pages.value.active.ifEmpty { null })
        _pages.value = list
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
            val old = _scene.value
            _scene.value = next
            viewport.reset()
            shown = id
            withContext(Dispatchers.Default) { runCatching(next::reload) }
            settings.activePage = id
            withContext(Dispatchers.Default) { open.showing(id) }
            old?.let { retire(it) }
        }
    }

    /** Closes a left page's scene a moment later: the renderer may still be drawing its last frame. */
    private fun retire(scene: PageScene) {
        retiring.add(scene)
        viewModelScope.launch {
            delay(RETIRE_DELAY_MS)
            if (retiring.remove(scene)) scene.close()
        }
    }

    /** Shows `id`. */
    fun selectPage(id: String) {
        val list = _pages.value
        if (id == list.active || list.page(id) == null) return
        _pages.value = list.copy(active = id)
        viewModelScope.launch { showActive() }
    }

    /**
     * Saves a new address/token and reconnects.
     *
     * A six-character code in the token field is a pairing code: it is exchanged for the real token (and the
     * server's URL) first, so nobody has to copy 43 characters by hand.
     */
    fun updateSync(
        url: String,
        token: String,
    ) {
        viewModelScope.launch {
            val trimmed = token.trim()
            val resolved =
                if (LibraryClient.looksLikeCode(trimmed)) {
                    val base = url.ifBlank { settings.displayedUrl() }
                    withContext(Dispatchers.IO) {
                        LibraryClient(base, "").pair(trimmed.replace("-", "").uppercase())
                    }
                } else {
                    null
                }
            settings.overrideUrl = resolved?.second?.ifBlank { url } ?: url
            settings.token = resolved?.first ?: token
            engine?.start()
        }
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
