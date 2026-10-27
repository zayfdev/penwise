package dev.penwise.ink

import android.app.Application
import android.util.Log
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import dev.penwise.BuildConfig
import dev.penwise.R
import dev.penwise.tools.ToolState
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import org.json.JSONObject

private const val HISTORY_SAVE_DELAY_MS = 400L

/**
 * Page and tool state that survives configuration changes; owns loading, saving and the link to the PC server.
 *
 * The tablet is offline-first: pages and strokes always persist locally, and the server hears about them when it
 * is reachable. Every change goes to disk and to the sync client fire-and-forget, so drawing never waits.
 */
class PageViewModel(
    application: Application,
) : AndroidViewModel(application) {
    val store = StrokeStore()
    val tools = ToolState()

    /** Pictures placed on the open page (screenshots, component cut-outs); drawn under the ink. */
    val images =
        PageImages { asset ->
            LibraryClient(settings.endpoints().firstOrNull().orEmpty(), settings.token).asset(asset)
        }
    private val settings = SyncSettings(application)
    private val library = PageLibrary(application.filesDir, settings)

    /** The library index; Compose observes it for the page strip. */
    private val _pages = MutableStateFlow(PageList.firstRun())

    /** Every page in the library, in reading order. */
    val pages: StateFlow<PageList> = _pages.asStateFlow()

    private val notebooks = NotebookActions(library, _pages, viewModelScope)
    private val session = PageSession(application.filesDir, _pages.value.active)

    /** Held while a page is being opened, so two switches never interleave their loads and swaps. */
    private val pageLock = Mutex()
    private var historySave: Job? = null
    private var sync = newClient()
    private var loading = false
    private var loaded = false

    /** Pages already offered to the server this session; one create per page, whatever `meta` frames follow. */
    private val offeredPages = HashSet<String>()

    /** The page currently on screen. */
    val activePageId: String get() = _pages.value.active

    init {
        // The file is read here, on the UI thread where the store changes, so it is always the store's own page.
        store.onAppend = { stroke ->
            val target = session.file
            viewModelScope.launch { target.append(stroke) }
            sync.sendStroke(stroke)
        }
        store.onRewrite = { strokes ->
            val target = session.file
            viewModelScope.launch { target.rewrite(strokes) }
        }
        store.onOp = { op -> sync.sendOp(op) }
        store.onHistoryChanged = ::scheduleHistorySave
    }

    /** Streams the in-progress stroke to the PC mirror. */
    val liveOut: LiveOut = SyncLiveOut { sync }

    /** URL shown in the settings dialog. */
    val syncUrl: String get() = settings.displayedUrl()

    /** Token shown in the settings dialog (empty when the server has none). */
    val syncToken: String get() = settings.token

    private fun newClient(): SyncClient =
        SyncClient(settings.endpoints()).also { client ->
            client.onFrame = ::onRemoteFrame
            client.onConnect = {
                // Ask for the server's page and diff against it; sending our whole page here would wipe
                // whatever the server gained while we were away (agent marks, other devices).
                sync.sendSelect(session.id)
                SyncService.start(
                    getApplication(),
                    getApplication<Application>().getString(R.string.sync_connected, settings.displayedUrl()),
                )
            }
        }

    /** Applies index and page frames pushed by the server. */
    private fun onRemoteFrame(text: String) {
        val frame = runCatching { JSONObject(text) }.getOrNull() ?: return
        when (frame.optString("t")) {
            "meta" -> {
                adoptMeta(frame.optJSONObject("meta"))
            }

            "page" -> {
                if (frame.optString("page") == session.id) applyRemotePage(frame)
            }

            "image" -> {
                if (frame.optString("page") == session.id) {
                    frame.optJSONObject("image")?.let(PageImage::read)?.let(images::add)
                }
            }

            "stroke" -> {
                if (frame.optString("page") == session.id) {
                    RemoteEdits.applyStroke(store, viewModelScope, frame) { session.id == frame.optString("page") }
                }
            }

            "op" -> {
                if (frame.optString("page") == session.id) {
                    RemoteEdits.applyOp(store, viewModelScope, frame.optJSONObject("op")) {
                        session.id == frame.optString("page")
                    }
                }
            }
        }
    }

    /**
     * Adopts the server's index, keeping the page on screen.
     *
     * A page this tablet has that the server does not is pushed up instead of being dropped: the page is
     * created server-side with the same id and its strokes are sent, so nothing drawn offline is lost.
     */
    private fun adoptMeta(meta: JSONObject?) {
        val list = meta?.let { runCatching { PageListCodec.read(it) }.getOrNull() } ?: return
        val open = activePageId
        when {
            list.page(open) == null -> pushLocalPage(open)
            else -> adoptIndex(list, open)
        }
    }

    /**
     * Sends a page the server has never seen, so adopting its index cannot drop local strokes.
     *
     * Once per page: a server that answers with a different id (it refuses `pg_local`) would otherwise announce
     * a new index, which still lacks our page, which pushes again. The id it chose is adopted instead.
     */
    private fun pushLocalPage(open: String) {
        val local = _pages.value.page(open) ?: return
        if (!offeredPages.add(open)) return
        val snapshot = store.snapshot()
        viewModelScope.launch {
            val response = withContext(Dispatchers.IO) { library.createRemote(local) }
            if (response == null) {
                offeredPages.remove(open)
                return@launch
            }
            val assigned = PageIdAdoption.idOf(response)
            if (assigned == null || assigned == open) {
                sync.sendPage(snapshot)
            } else {
                pageLock.withLock { adoptServerId(open, assigned) }
            }
        }
    }

    /** Renames page `from` (files, index, open session) to the id the server minted for it. */
    private suspend fun adoptServerId(
        from: String,
        to: String,
    ) {
        val renamed = PageIdAdoption.renamed(_pages.value, from, to)
        _pages.value = renamed
        offeredPages.add(to)
        val wasOpen = session.id == from
        // Re-point first, then queue the move on the page writer: appends queued for the old name run before the
        // move, and every later one targets the new name.
        if (wasOpen) {
            session.rename(to)
            sync.pageId = to
            pushedSeeds.clear()
        }
        val dir = getApplication<Application>().filesDir
        withContext(PageIo) { PageIdAdoption.moveFiles(dir, from, to) }
        withContext(Dispatchers.IO) { library.save(renamed) }
        if (wasOpen) sync.sendSelect(to)
    }

    /** Takes the server's index; switches pages when the open one is gone from it. */
    private fun adoptIndex(
        list: PageList,
        open: String,
    ) {
        val keep = list.page(open)?.id ?: list.startPage()?.id ?: return
        val adopted = if (keep == list.active) list else list.copy(active = keep)
        if (keep == open && adopted.all().map { it.id } == _pages.value.all().map { it.id }) return
        _pages.value = adopted
        viewModelScope.launch(Dispatchers.IO) { library.save(adopted) }
        if (keep != open) viewModelScope.launch { showActivePage() }
    }

    /**
     * Applies a server page snapshot: merge what we lack, then offer the server whatever it lacks.
     *
     * The second half is what makes offline drawing work without ever replacing the server's page — a stroke
     * the tablet has and the server does not is sent once, by seed.
     */
    private fun applyRemotePage(frame: JSONObject) {
        val incoming = Wire.parsePage(frame.toString()) ?: return
        frame.optJSONArray("images")?.let { array ->
            images.replace(
                (0 until array.length()).mapNotNull { index ->
                    array.optJSONObject(index)?.let(PageImage::read)
                },
            )
        }
        val pageId = session.id
        viewModelScope.launch {
            val ready = withContext(Dispatchers.Default) { StrokeRebuild.usable(incoming) }
            // The user may have moved to another page while the geometry was built; its strokes are not ours.
            if (session.id != pageId) return@launch
            if (store.merge(ready)) {
                Log.i("PenwiseSync", "merged ${ready.size} remote strokes")
                store.persist()
            }
            val serverSeeds = ready.mapTo(HashSet()) { it.seed }
            val missing =
                store
                    .snapshot()
                    .filter { it.seed !in serverSeeds && pushedSeeds.add(it.seed) }
            if (missing.isNotEmpty()) {
                Log.i("PenwiseSync", "pushing ${missing.size} local strokes the server lacks")
                missing.forEach(sync::sendStroke)
            }
        }
    }

    /**
     * Saves a new address/token and reconnects, resending the page so the mirror is immediately correct.
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
            applySync(resolved?.second?.ifBlank { url } ?: url, resolved?.first ?: token)
        }
    }

    private fun applySync(
        url: String,
        token: String,
    ) {
        settings.overrideUrl = url
        settings.token = token
        pushedSeeds.clear()
        sync.stop()
        sync = newClient().also { it.pageId = session.id }
        sync.start()
    }

    /** Loads the library and the active page, rebuilds its geometry off the main thread, then starts syncing. */
    fun loadPage() {
        if (loading || loaded) return
        loading = true
        viewModelScope.launch {
            try {
                val list = withContext(Dispatchers.IO) { library.load() }
                val start = list.startPage() ?: PageList.firstRun().startPage() ?: return@launch
                _pages.value = list.copy(active = start.id)
                pageLock.withLock { openPage(start.id) }
                loaded = true
                sync.start()
                if (pendingFixture != null) startPendingReplay()
            } finally {
                loading = false
            }
        }
    }

    /** Saves the visible page, then shows `id` with an empty undo history. */
    fun selectPage(id: String) {
        if (id == activePageId || _pages.value.page(id) == null) return
        _pages.value = _pages.value.copy(active = id)
        viewModelScope.launch { showActivePage() }
    }

    /**
     * Opens the page the index marks active, if it is not open already.
     *
     * Every switch goes through here under [pageLock], and the target is read once the lock is held: of several
     * quick taps only the last one loads anything, and a load never overlaps another load or its swap.
     */
    private suspend fun showActivePage() {
        pageLock.withLock {
            withContext(Dispatchers.IO) { library.save(_pages.value) }
            val id = activePageId
            if (session.id != id) {
                openPage(id)
                sync.sendSelect(id)
            }
        }
    }

    /**
     * Creates a page locally, switches to it, and tells the server; returns the new page's id so the caller can
     * navigate immediately. `notebook` defaults to the active page's.
     */
    fun newPage(
        title: String,
        template: PageTemplate,
        notebook: String? = null,
    ): String {
        val list = _pages.value
        val page =
            PageMeta(
                id = library.newPageId(),
                title = title.ifBlank { list.nextTitle() },
                infinite = template.infinite,
                background = template.background,
            )
        // The switch is local and immediate: the server is told afterwards, so a slow or absent server cannot
        // leave the strip showing one page while ink still lands in another.
        _pages.value = list.withPage(page, notebook).copy(active = page.id)
        viewModelScope.launch {
            showActivePage()
            withContext(Dispatchers.IO) { library.createRemote(page) }
            if (session.id == page.id) sync.sendSelect(page.id)
        }
        return page.id
    }

    /** Seeds already offered to the server for the open page, so a reconnect does not resend them. */
    private val pushedSeeds = HashSet<Long>()

    /** Pending debug replay, run once the page has finished loading. */
    private var pendingFixture: String? = null

    /**
     * Debug builds only: replays a recorded page onto the visible one. Deferred until [loadPage] finishes, because
     * restoring the page file would otherwise replace the replayed strokes.
     */
    fun requestReplay(path: String) {
        if (!BuildConfig.DEBUG) return
        pendingFixture = path
        if (!loading) startPendingReplay()
    }

    /** Appends a recorded page's strokes through the normal path — store, file, sync — so it is a real test. */
    private fun startPendingReplay(): Int {
        val path = pendingFixture ?: return 0
        pendingFixture = null
        if (!BuildConfig.DEBUG) return 0
        // `add` already persists and syncs; invoking the hook here too sent every stroke twice.
        val added = FixtureReplay.replay(getApplication(), path) { stroke -> store.add(stroke) }
        store.onPageChanged?.invoke()
        return added
    }

    /** Renames a page locally and on the server. */
    fun renamePage(
        id: String,
        title: String,
    ) {
        val page = _pages.value.page(id) ?: return
        val renamed = page.copy(title = title.ifBlank { page.title })
        val list = _pages.value.withUpdated(renamed)
        _pages.value = list
        viewModelScope.launch {
            withContext(Dispatchers.IO) {
                library.save(list)
                library.renameRemote(id, renamed.title)
            }
        }
    }

    /** Creates a notebook locally and on the server. */
    fun createNotebook(title: String) = notebooks.create(title)

    /** Renames a notebook. */
    fun renameNotebook(
        id: String,
        title: String,
    ) = notebooks.rename(id, title)

    /** Deletes an empty notebook (the last one and non-empty ones are refused). */
    fun deleteNotebook(id: String) = notebooks.delete(id)

    /** Deletes a page (never the last one) locally and on the server. */
    fun deletePage(id: String) {
        val before = _pages.value
        val after = before.withoutPage(id)
        if (after.all().size == before.all().size) return
        _pages.value = after
        viewModelScope.launch {
            // Open the survivor first: only then does the screen, the store and the file agree again.
            showActivePage()
            withContext(Dispatchers.IO) { library.deleteRemote(id) }
        }
    }

    /**
     * Loads `id`'s strokes and saved undo history, then swaps them in. Callers hold [pageLock].
     *
     * Everything slow happens before the swap and touches no shared state; the swap itself never suspends, so the
     * store, the page file, the undo file and the sync scope change together.
     */
    private suspend fun openPage(id: String) {
        val page = session.load(id)
        if (session.id != id) saveLeavingHistory()
        images.clear()
        session.adopt(page)
        pushedSeeds.clear()
        sync.pageId = id
        store.restore(page.strokes)
        store.restoreHistory(page.undo, page.redo)
        // Strokes from the oldest files have no seed; give them one so each can be addressed and removed.
        val reseeded = store.assignMissingSeeds()
        if (reseeded.isNotEmpty()) {
            Log.i("PenwiseSync", "gave ${reseeded.size} legacy strokes a seed")
            reseeded.forEach(sync::sendStroke)
        }
        store.onPageChanged?.invoke()
        Log.i("PenwiseSync", "page $id loaded: ${page.strokes.size} strokes, ${page.undo.size} undo steps")
    }

    /** Writes the page being left its pending undo history now; the debounce would otherwise be cancelled. */
    private fun saveLeavingHistory() {
        historySave?.cancel()
        val target = session.undoFile
        val (undo, redo) = store.historySnapshot()
        viewModelScope.launch { target.save(undo, redo) }
    }

    /** Writes the undo history after a short quiet period: drawing must never wait on a file. */
    private fun scheduleHistorySave() {
        historySave?.cancel()
        historySave =
            viewModelScope.launch {
                delay(HISTORY_SAVE_DELAY_MS)
                // Snapshot and file are read together on the UI thread, so they always belong to one page.
                val target = session.undoFile
                val (undo, redo) = store.historySnapshot()
                target.save(undo, redo)
            }
    }
}
