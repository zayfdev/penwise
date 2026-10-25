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
import kotlinx.coroutines.withContext
import org.json.JSONObject

private const val HISTORY_SAVE_DELAY_MS = 400L

/** A page template the new-page dialog offers. */
enum class PageTemplate(
    val label: String,
    val hint: String,
    val background: String,
    val infinite: Boolean,
) {
    Blank("Blank", "plain A4", "blank", false),
    Ruled("Ruled", "lines", "ruled", false),
    Grid("Grid", "squares", "grid", false),
    Infinite("Infinite", "unbounded", "blank", true),
}

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

    private var file = library.strokes(_pages.value.active)
    private var undoFile = UndoHistoryFile(getApplication<Application>().filesDir, _pages.value.active)
    private var historySave: Job? = null
    private var sync = newClient()
    private var loading = false

    /** The page currently on screen. */
    val activePageId: String get() = _pages.value.active

    init {
        store.onAppend = { stroke ->
            viewModelScope.launch { file.append(stroke) }
            sync.sendStroke(stroke)
        }
        store.onRewrite = { strokes ->
            viewModelScope.launch { file.rewrite(strokes) }
        }
        store.onOp = { op -> send(op) }
        store.onHistoryChanged = ::scheduleHistorySave
    }

    /** Streams the in-progress stroke to the PC mirror. */
    val liveOut: LiveOut =
        object : LiveOut {
            override fun begin(
                preset: Int,
                argb: Int,
                scale: Float,
                seed: Long,
            ) = sync.sendLiveBegin(preset, argb, scale, seed)

            override fun samples(
                samples: FloatArray,
                times: IntArray,
                count: Int,
            ) = sync.sendLiveSamples(samples, times, count)

            override fun end(cancelled: Boolean) {
                if (cancelled) sync.sendLiveCancel()
            }

            override fun shape(
                preset: Int,
                argb: Int,
                scale: Float,
                seed: Long,
                samples: FloatArray,
                times: IntArray,
                count: Int,
            ) = sync.sendShapePreview(preset, argb, scale, seed, samples, times, count)

            override fun erasing(
                x: Float,
                y: Float,
                radius: Float,
            ) = sync.sendErasePreview(x, y, radius)
        }

    /** Sends one incremental edit instead of the whole page. */
    private fun send(op: PageOp) {
        when (op) {
            is PageOp.Add -> op.strokes.forEach(sync::sendStroke)
            is PageOp.Remove -> sync.sendRemove(op.seeds)
            is PageOp.Replace -> sync.sendReplace(op.seeds, op.fragments)
            is PageOp.Clear -> sync.sendClear()
        }
    }

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
                sync.sendSelect(activePageId)
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
                if (frame.optString("page") == activePageId) applyRemotePage(frame)
            }

            "image" -> {
                if (frame.optString("page") == activePageId) {
                    frame.optJSONObject("image")?.let(PageImage::read)?.let(images::add)
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

    /** Sends a page the server has never seen, so adopting its index cannot drop local strokes. */
    private fun pushLocalPage(open: String) {
        val local = _pages.value.page(open) ?: return
        viewModelScope.launch(Dispatchers.IO) {
            library.createRemote(local)
            sync.sendPage(store.snapshot())
        }
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
        if (keep != open) {
            viewModelScope.launch {
                withContext(Dispatchers.IO) { file.rewrite(store.snapshot()) }
                openPage(keep)
                sync.sendSelect(keep)
            }
        }
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
        viewModelScope.launch {
            val ready =
                withContext(Dispatchers.Default) {
                    incoming.forEach { if (it.geometry == null) it.rebuild() }
                    incoming
                }
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
        sync = newClient()
        sync.start()
    }

    /** Loads the library and the active page, rebuilds its geometry off the main thread, then starts syncing. */
    fun loadPage() {
        if (loading) return
        loading = true
        viewModelScope.launch {
            val list = withContext(Dispatchers.IO) { library.load() }
            val start = list.startPage() ?: PageList.firstRun().startPage() ?: return@launch
            _pages.value = list.copy(active = start.id)
            openPage(start.id)
            sync.start()
            loading = false
            if (pendingFixture != null) startPendingReplay()
        }
    }

    /** Saves the visible page, then shows `id` with an empty undo history. */
    fun selectPage(id: String) {
        if (id == activePageId || _pages.value.page(id) == null) return
        val leaving = store.snapshot()
        val next = _pages.value.copy(active = id)
        _pages.value = next
        viewModelScope.launch {
            withContext(Dispatchers.IO) {
                file.rewrite(leaving)
                library.save(next)
            }
            openPage(id)
            sync.sendSelect(id)
        }
    }

    /**
     * Creates a page locally, tells the server, and switches to it; returns the new page's id so the caller can
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
        // Add the page first, then switch to it: `selectPage` is what saves the page being left, loads the new
        // (empty) file, clears undo history and tells the server which page this session shows.
        _pages.value = list.withPage(page, notebook)
        viewModelScope.launch {
            withContext(Dispatchers.IO) {
                library.save(_pages.value)
                library.createRemote(page)
            }
            selectPage(page.id)
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
    fun createNotebook(title: String) {
        val trimmed = title.trim().ifBlank { "Notebook ${_pages.value.books.size + 1}" }
        val book = PageBook(library.newNotebookId(), trimmed, emptyList())
        val list = _pages.value.withNotebook(book.id, book.title)
        _pages.value = list
        viewModelScope.launch(Dispatchers.IO) {
            library.save(list)
            library.createNotebookRemote(book.title)
        }
    }

    /** Renames a notebook. */
    fun renameNotebook(
        id: String,
        title: String,
    ) {
        val trimmed = title.trim()
        if (trimmed.isEmpty()) return
        val list = _pages.value.withRenamedNotebook(id, trimmed)
        _pages.value = list
        viewModelScope.launch(Dispatchers.IO) {
            library.save(list)
            library.renameNotebookRemote(id, trimmed)
        }
    }

    /** Deletes an empty notebook (the last one and non-empty ones are refused). */
    fun deleteNotebook(id: String) {
        val before = _pages.value
        val after = before.withoutNotebook(id)
        if (after.books.size == before.books.size) return
        _pages.value = after
        viewModelScope.launch(Dispatchers.IO) {
            library.save(after)
            library.deleteNotebookRemote(id)
        }
    }

    /** Deletes a page (never the last one) locally and on the server. */
    fun deletePage(id: String) {
        val before = _pages.value
        val after = before.withoutPage(id)
        if (after.all().size == before.all().size) return
        _pages.value = after
        viewModelScope.launch {
            withContext(Dispatchers.IO) {
                library.save(after)
                library.deleteRemote(id)
            }
            if (id == before.active) {
                openPage(after.active)
                sync.sendSelect(after.active)
            }
        }
    }

    /** Loads `id`'s strokes and saved undo history. */
    private suspend fun openPage(id: String) {
        images.clear()
        file = library.strokes(id)
        undoFile = UndoHistoryFile(getApplication<Application>().filesDir, id)
        pushedSeeds.clear()
        sync.pageId = id
        val saved = withContext(Dispatchers.IO) { file.load() }
        withContext(Dispatchers.Default) { saved.forEach { it.rebuild() } }
        store.restore(saved)
        val (undo, redo) = withContext(Dispatchers.IO) { undoFile.load() }
        withContext(Dispatchers.Default) { rebuildHistory(undo + redo) }
        store.restoreHistory(undo, redo)
        store.onPageChanged?.invoke()
        Log.i("PenwiseSync", "page $id loaded: ${saved.size} strokes, ${undo.size} undo steps")
    }

    /** Every stroke a persisted step refers to needs its geometry before undo can paint it. */
    private fun rebuildHistory(steps: List<HistoryStep>) {
        steps.forEach { step ->
            when (step) {
                is HistoryStep.Added -> {
                    if (step.stroke.geometry == null) step.stroke.rebuild()
                }

                is HistoryStep.Removed -> {
                    step.removed.forEach { (_, stroke) ->
                        if (stroke.geometry ==
                            null
                        ) {
                            stroke.rebuild()
                        }
                    }
                }

                is HistoryStep.Replaced -> {
                    (step.removed + step.added).forEach { if (it.geometry == null) it.rebuild() }
                }
            }
        }
    }

    /** Writes the undo history after a short quiet period: drawing must never wait on a file. */
    private fun scheduleHistorySave() {
        historySave?.cancel()
        historySave =
            viewModelScope.launch(Dispatchers.IO) {
                delay(HISTORY_SAVE_DELAY_MS)
                val (undo, redo) = store.historySnapshot()
                undoFile.save(undo, redo)
            }
    }
}
