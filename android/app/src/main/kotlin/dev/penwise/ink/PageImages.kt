package dev.penwise.ink

import android.graphics.Bitmap
import android.util.Log
import android.util.LruCache
import java.util.concurrent.Executors

/**
 * Decoded bitmaps of the pictures a page shows, keyed by asset hash (64 hex chars, as the scene's image parts name
 * them). Which pictures a page has, and where, is scene data; this class only fetches and keeps the pixels.
 *
 * Decoding happens on one background thread; the render thread only ever reads a bitmap that is already decoded
 * ([bitmap] asks for a missing one and returns null), and repaints when [onChanged] fires.
 */
class PageImages(
    private val fetch: (String) -> Bitmap?,
) {
    private val cache =
        object : LruCache<String, Bitmap>(MAX_BYTES) {
            override fun sizeOf(
                key: String,
                value: Bitmap,
            ): Int = value.byteCount
        }
    private val loading = HashSet<String>()
    private val failed = HashSet<String>()
    private val worker =
        Executors.newSingleThreadExecutor { runnable ->
            Thread(runnable, "penwise-images").apply { isDaemon = true }
        }

    /** Fires after a picture decodes, so the surface can repaint. */
    @Volatile
    var onChanged: (() -> Unit)? = null

    /** The decoded bitmap, or null while it loads (a load is started if none is running). */
    fun bitmap(asset: String): Bitmap? {
        cache.get(asset)?.let { return it }
        if (claim(asset)) decode(asset)
        return null
    }

    /** Forgets every decoded bitmap and failure (another server, or memory pressure). */
    fun clear() {
        cache.evictAll()
        synchronized(loading) { failed.clear() }
    }

    /** True when this call is the one that will load [asset]; assets that failed once are not retried until [clear]. */
    private fun claim(asset: String): Boolean = synchronized(loading) { asset !in failed && loading.add(asset) }

    private fun decode(asset: String) {
        worker.execute {
            val bitmap = runCatching { fetch(asset) }.getOrNull()
            if (bitmap != null) {
                cache.put(asset, bitmap)
            } else {
                Log.w(TAG, "picture $asset did not load")
                synchronized(loading) { failed.add(asset) }
            }
            synchronized(loading) { loading.remove(asset) }
            if (bitmap != null) onChanged?.invoke()
        }
    }

    private companion object {
        const val TAG = "PenwiseImages"
        const val MAX_BYTES = 48 * 1024 * 1024
    }
}
