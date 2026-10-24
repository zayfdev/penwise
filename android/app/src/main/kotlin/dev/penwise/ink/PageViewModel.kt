package dev.penwise.ink

import android.app.Application
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import dev.penwise.tools.ToolState
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/** Page and tool state that survives configuration changes; owns loading and saving the page. */
class PageViewModel(
    application: Application,
) : AndroidViewModel(application) {
    val store = StrokeStore()
    val tools = ToolState()
    private val file = StrokeFile(application.filesDir)
    private var loading = false

    init {
        store.onAppend = { stroke -> viewModelScope.launch { file.append(stroke) } }
        store.onRewrite = { strokes -> viewModelScope.launch { file.rewrite(strokes) } }
    }

    /** Loads the saved page once and rebuilds its geometry off the main thread. */
    fun loadPage() {
        if (loading) return
        loading = true
        viewModelScope.launch {
            val saved = file.load()
            withContext(Dispatchers.Default) { saved.forEach { it.rebuild() } }
            store.restore(saved)
            store.onPageChanged?.invoke()
        }
    }
}
