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
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeoutOrNull
import java.security.SecureRandom

private const val TAG = "PenwiseSync"
private const val FIRST_SYNC_TIMEOUT_MS = 4_000L
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

    private val _status = MutableStateFlow(SyncStatus.STOPPED)

    /** State of the link to the PC server. */
    val status: StateFlow<SyncStatus> = _status.asStateFlow()

    private val firstSync = CompletableDeferred<Unit>()
    private val pageLock = Mutex()
    private val retiring = ArrayList<PageScene>()
    private var started = false

    @Volatile
    private var ready = false

    @Volatile
    private var shown = ""

    /** The page the server said was current at the last `Welcome`; only used when nothing was open before. */
    @Volatile
    private var serverPage: String? = null
    private var pendingFixture: String? = null

    private val engine =
        SyncEngine(
            filesDir = application.filesDir,
            endpoints = settings::endpoints,
            token = { settings.token },
            listener = Listener(),
        )

    /** Streams the in-progress stroke to the PC mirror. */
    val liveOut: LiveOut = SyncLiveOut(engine.core, engine::send)

    /** URL shown in the settings dialog. */
    val syncUrl: String get() = settings.displayedUrl()

    /** Token shown in the settings dialog (empty when the server has none). */
    val syncToken: String get() = settings.token

    /** Ops waiting for the server's acknowledgement. */
    fun pendingOps(): Int = engine.core.outbox().pending

    private inner class Listener : SyncListener {
        override fun onOutcome(outcome: Outcome) {
            if (outcome.kind == Kind.WELCOME) serverPage = outcome.page?.let(Ids::hex)
            if (!ready) return
            if (outcome.catalogChanged) publishCatalog()
            if (outcome.hasChanges) {
                try {
                    _scene.value?.refresh(outcome.raw)
                } catch (_: IllegalStateException) {
                    Log.i(TAG, "scene closed while a frame arrived") // the page was just left.
                }
            }
        }

        override fun onStatus(status: SyncStatus) {
            _status.value = status
            if (status == SyncStatus.ONLINE) {
                val app = getApplication<Application>()
                SyncService.start(app, app.getString(R.string.sync_connected, settings.displayedUrl()))
            }
        }

        override fun onCaughtUp() {
            firstSync.complete(Unit)
        }

        override fun onUnreachable() {
            firstSync.complete(Unit)
        }
    }

    /**
     * Opens the library and the active page, then connects. First launch after the upgrade imports the old page
     * files once, after the first catch-up with the server (their strokes carry deterministic ids, so strokes the
     * server already migrated are skipped).
     */
    fun loadPage() {
        if (started) return
        started = true
        viewModelScope.launch {
            engine.start()
            withContext(Dispatchers.IO) { prepareLibrary() }
            ready = true
            val list = withContext(Dispatchers.IO) { engine.library.list(settings.activePage ?: serverPage) }
            _pages.value = list
            showActive()
            pendingFixture?.let { runReplay(it) }
            pendingFixture = null
        }
    }

    /** Waits for the server when the catalog could still gain content from it, then imports and seeds. */
    private suspend fun prepareLibrary() {
        val empty =
            engine.library
                .list(null)
                .all()
                .isEmpty()
        if (empty || engine.legacy.pending()) withTimeoutOrNull(FIRST_SYNC_TIMEOUT_MS) { firstSync.await() }
        if (engine.legacy.pending()) importLegacy()
        if (engine.library
                .list(null)
                .all()
                .isEmpty()
        ) {
            engine.library.createDefault()
        }
    }

    // Native errors and panics surface as RuntimeException.
    @Suppress("TooGenericExceptionCaught")
    private fun importLegacy() {
        try {
            val report = engine.legacy.run(engine.core)
            engine.flush()
            engine.legacy.done()
            Log.i(TAG, "legacy import: $report")
        } catch (error: RuntimeException) {
            // Not marked done: the next launch retries (everything dedupes by deterministic id).
            Log.e(TAG, "legacy import failed: ${error.message}")
        }
    }

    /** Re-reads the catalog; moves to another page when the open one is gone. */
    private fun publishCatalog(active: String? = null) {
        val list = engine.library.list(active ?: _pages.value.active.ifEmpty { null })
        _pages.value = list
        if (list.active != shown) viewModelScope.launch { showActive() }
    }

    /** Opens the page the catalog marks active, if it is not open already. Serialized: only the last switch loads. */
    private suspend fun showActive() {
        pageLock.withLock {
            val id = _pages.value.active
            val bytes = Ids.bytes(id)
            if (bytes == null || id == shown) return
            val core = engine.core
            val next =
                withContext(Dispatchers.Default) {
                    PageScene(core.handle, core.lock, bytes, random.nextLong(), core)
                }
            val old = _scene.value
            _scene.value = next
            shown = id
            settings.activePage = id
            engine.showing(id)
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
     * Creates a page and switches to it; returns its id so the caller can navigate at once. `notebook` defaults to
     * the first one.
     */
    fun newPage(
        title: String,
        template: PageTemplate,
        notebook: String? = null,
    ): String? {
        val list = _pages.value
        val target = notebook ?: list.books.firstOrNull { book -> book.pages.any { it.id == list.active } }?.id
        val id = engine.library.newPage(title, template, target) ?: return null
        publishCatalog(active = id)
        return id
    }

    fun renamePage(
        id: String,
        title: String,
    ) {
        engine.library.renamePage(id, title)
        publishCatalog()
    }

    /** Deletes a page (never the last one). */
    fun deletePage(id: String) {
        if (engine.library.deletePage(id)) publishCatalog()
    }

    fun createNotebook(title: String) {
        engine.library.newNotebook(title)
        publishCatalog()
    }

    fun renameNotebook(
        id: String,
        title: String,
    ) {
        engine.library.renameNotebook(id, title)
        publishCatalog()
    }

    /** Deletes an empty notebook (the last one and non-empty ones are refused). */
    fun deleteNotebook(id: String) {
        if (engine.library.deleteNotebook(id)) publishCatalog()
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
            engine.start()
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
        withContext(Dispatchers.Default) {
            FixtureReplay.replay(getApplication(), path) { stroke ->
                val edits =
                    open.addStroke(
                        open.newItemId(),
                        stroke.preset,
                        stroke.argb,
                        stroke.scale,
                        stroke.seed,
                        stroke.samples,
                        stroke.times,
                    )
                open.commit(edits)
            }
        }
    }

    /** Writes the sync state file now (the app is going to the background and may be killed). */
    fun flushState() {
        viewModelScope.launch(Dispatchers.IO) { engine.flush() }
    }

    override fun onCleared() {
        _scene.value?.close()
        retiring.forEach(PageScene::close)
        engine.close()
        super.onCleared()
    }
}
