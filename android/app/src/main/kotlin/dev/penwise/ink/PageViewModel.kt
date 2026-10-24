package dev.penwise.ink

import android.app.Application
import android.util.Log
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import dev.penwise.R
import dev.penwise.tools.ToolState
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.json.JSONObject

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
    private val settings = SyncSettings(application)
    private val library = PageLibrary(application.filesDir, settings)

    /** The library index; Compose observes it for the page strip. */
    private val _pages = MutableStateFlow(PageList.firstRun())

    /** Every page in the library, in reading order. */
    val pages: StateFlow<PageList> = _pages.asStateFlow()

    private var file = library.strokes(_pages.value.active)
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
                sync.sendSelect(activePageId)
                sync.sendPage(store.snapshot())
            }
        }

    /** Applies index and page frames pushed by the server. */
    private fun onRemoteFrame(text: String) {
        val frame = runCatching { JSONObject(text) }.getOrNull() ?: return
        when (frame.optString("t")) {
            "meta" -> adoptMeta(frame.optJSONObject("meta"))
            "page" -> if (frame.optString("page") == activePageId) applyRemotePage(frame)
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

    /** Merges a server page snapshot into the store (agent marks, other devices). */
    private fun applyRemotePage(frame: JSONObject) {
        val incoming = Wire.parsePage(frame.toString()) ?: return
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
        }
    }

    /** Saves a new address/token and reconnects, resending the page so the mirror is immediately correct. */
    fun updateSync(
        url: String,
        token: String,
    ) {
        settings.overrideUrl = url
        settings.token = token
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

    /** Creates a page locally, tells the server, and shows it. */
    fun newPage(
        title: String,
        template: PageTemplate,
    ) {
        val list = _pages.value
        val page =
            PageMeta(
                id = library.newPageId(),
                title = title.ifBlank { list.nextTitle() },
                infinite = template.infinite,
                background = template.background,
            )
        val withPage = list.withPage(page)
        _pages.value = withPage.copy(active = page.id)
        viewModelScope.launch {
            withContext(Dispatchers.IO) {
                library.save(withPage)
                library.createRemote(page)
            }
            selectPage(page.id)
        }
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

    /** Loads `id`'s strokes and hands them to the store; clears undo history. */
    private suspend fun openPage(id: String) {
        file = library.strokes(id)
        sync.pageId = id
        val saved = withContext(Dispatchers.IO) { file.load() }
        withContext(Dispatchers.Default) { saved.forEach { it.rebuild() } }
        store.restore(saved)
        store.onPageChanged?.invoke()
        Log.i("PenwiseSync", "page $id loaded: ${saved.size} strokes")
    }
}
