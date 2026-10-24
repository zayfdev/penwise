package dev.penwise.ink

import android.app.Application
import android.util.Log
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import dev.penwise.R
import dev.penwise.tools.ToolState
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/**
 * Page and tool state that survives configuration changes; owns loading, saving and the link to the PC server.
 *
 * Every change goes to disk and to the sync client. Both are fire-and-forget, so drawing never waits on either.
 */
class PageViewModel(
    application: Application,
) : AndroidViewModel(application) {
    val store = StrokeStore()
    val tools = ToolState()
    private val file = StrokeFile(application.filesDir)
    private val sync = SyncClient(application.resources.getStringArray(R.array.sync_urls).toList())
    private var loading = false

    init {
        store.onAppend = { stroke ->
            viewModelScope.launch { file.append(stroke) }
            sync.sendStroke(stroke)
        }
        store.onRewrite = { strokes ->
            viewModelScope.launch { file.rewrite(strokes) }
            sync.sendPage(strokes)
        }
    }

    /** Loads the saved page once, rebuilds its geometry off the main thread, then mirrors it to the PC. */
    fun loadPage() {
        if (loading) return
        loading = true
        viewModelScope.launch {
            val saved = file.load()
            withContext(Dispatchers.Default) { saved.forEach { it.rebuild() } }
            store.restore(saved)
            store.onPageChanged?.invoke()
            Log.i("PenwiseSync", "page loaded: ${saved.size} strokes")
            sync.start()
            sync.sendPage(saved)
        }
    }
}
