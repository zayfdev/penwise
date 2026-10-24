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
    private val settings = SyncSettings(application)
    private var sync = newClient()
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
        }

    /** URL shown in the settings dialog. */
    val syncUrl: String get() = settings.displayedUrl()

    /** Token shown in the settings dialog (empty when the server has none). */
    val syncToken: String get() = settings.token

    private fun newClient(): SyncClient =
        SyncClient(settings.endpoints()).also { client -> client.onFrame = ::onRemoteFrame }

    /** Applies page snapshots pushed by the server: agent marks and strokes from other devices. */
    private fun onRemoteFrame(text: String) {
        val incoming = Wire.parsePage(text) ?: return
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
        sync.sendPage(store.snapshot())
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
