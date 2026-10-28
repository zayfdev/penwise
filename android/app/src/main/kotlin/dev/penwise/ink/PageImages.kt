package dev.penwise.ink

import android.content.res.Resources
import android.graphics.Bitmap
import android.util.Log
import android.util.LruCache
import java.util.concurrent.Executor
import java.util.concurrent.Executors

/**
 * Decoded bitmaps of the pictures a page shows, keyed by asset hash (64 hex chars, as the scene's image parts name
 * them). Which pictures a page has, and where, is scene data; this class only fetches and keeps the pixels.
 *
 * Decoding happens on one background thread; the render thread only ever reads a bitmap that is already decoded
 * ([bitmap] asks for a missing one and returns null), and repaints when [onChanged] fires. [fetch] receives the pixel
 * budget one picture may use ([maxPixels]) and must downsample to it, so that a few pictures always fit the cache
 * together and a miss is never caused by the picture being bigger than the cache.
 *
 * A picture that failed to load is tried again once [RETRY_MS] have passed, at the next time the page asks for it.
 */
class PageImages(
    private val maxPixels: Int = screenPixels() * BUDGET_SCREENS,
    private val clock: () -> Long = System::currentTimeMillis,
    private val worker: Executor = decoder(),
    private val fetch: (asset: String, maxPixels: Int) -> Bitmap?,
) {
    private val cache =
        object : LruCache<String, Bitmap>(maxPixels.toLong().times(CACHED_PICTURES * BYTES_PER_PIXEL).toInt()) {
            override fun sizeOf(
                key: String,
                value: Bitmap,
            ): Int = value.byteCount
        }
    private val loading = HashSet<String>()

    /** Asset to the time it failed (ms); an entry older than [RETRY_MS] no longer blocks a new load. */
    private val failed = HashMap<String, Long>()

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

    /** True when this call is the one that will load [asset]; a recent failure is not retried yet. */
    internal fun claim(asset: String): Boolean =
        synchronized(loading) {
            val failedAt = failed[asset]
            if (failedAt != null) {
                if (clock() - failedAt < RETRY_MS) return false
                failed.remove(asset)
            }
            loading.add(asset)
        }

    private fun decode(asset: String) {
        worker.execute {
            val bitmap = runCatching { fetch(asset, maxPixels) }.getOrNull()
            if (bitmap != null) {
                cache.put(asset, bitmap)
            } else {
                Log.w(TAG, "picture $asset did not load")
                synchronized(loading) { failed[asset] = clock() }
            }
            synchronized(loading) { loading.remove(asset) }
            if (bitmap != null) onChanged?.invoke()
        }
    }

    internal companion object {
        private const val TAG = "PenwiseImages"
        private const val BUDGET_SCREENS = 2
        private const val CACHED_PICTURES = 3
        private const val BYTES_PER_PIXEL = 4

        /** How long a failed picture stays failed before the next request for it tries again. */
        const val RETRY_MS = 10_000L

        private fun decoder(): Executor =
            Executors.newSingleThreadExecutor { runnable ->
                Thread(runnable, "penwise-images").apply { isDaemon = true }
            }

        private fun screenPixels(): Int {
            val metrics = Resources.getSystem().displayMetrics
            return metrics.widthPixels * metrics.heightPixels
        }

        /**
         * The `inSampleSize` (a power of two, at least 1) that brings a [width] x [height] picture to at most
         * [maxPixels] pixels, so it can be decoded without ever holding the full-size bitmap.
         */
        fun sampleSize(
            width: Int,
            height: Int,
            maxPixels: Int,
        ): Int {
            var sample = 1
            while (width.toLong() * height / (sample.toLong() * sample) > maxPixels) sample *= 2
            return sample
        }
    }
}
