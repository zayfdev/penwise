package dev.penwise.frames

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.util.LruCache
import java.io.File
import java.util.concurrent.ExecutorService
import java.util.concurrent.Executors
import java.util.concurrent.RejectedExecutionException
import kotlin.math.max

/**
 * The strips of the stills as bitmaps. A strip's URL is content-addressed (`/frames/{id}/strip/{hash}.png`, immutable),
 * so it is the cache key: a new still reuses every strip whose content did not change. PNGs are kept on disk (one file
 * per strip), decoded off the main thread at the resolution the screen needs, and held in a memory cache bounded by
 * bytes. A strip is decoded again when the camera zooms in past the resolution it was decoded at.
 */
internal class StillBitmaps(
    private val dir: File,
    private val client: () -> StillClient?,
    private val onLoaded: () -> Unit,
    maxBytes: Int = DEFAULT_BYTES,
) {
    /** A decoded strip; [wanted] is the shrink the screen asked for, [sample] what the GPU limit allowed. */
    private class Entry(
        val bitmap: Bitmap,
        val sample: Int,
        val wanted: Int,
    )

    private val cache =
        object : LruCache<String, Entry>(maxBytes) {
            override fun sizeOf(
                key: String,
                value: Entry,
            ): Int = value.bitmap.allocationByteCount
        }
    private val inFlight = HashSet<String>()
    private val failedAt = HashMap<String, Long>()
    private val files = StripStore(dir)
    private val lock = Any()
    private val pool: ExecutorService = Executors.newFixedThreadPool(DECODE_THREADS) { Thread(it, "penwise-stills") }

    /** The decoded strip if there is one (any resolution), for the draw; never decodes and never allocates. */
    fun peek(url: String): Bitmap? = cache.get(url)?.bitmap

    /**
     * Makes sure [strip] is decoded for a view [targetWidthPx] wide: nothing when the cached one is fine enough,
     * otherwise a background download and decode. [onLoaded] runs (on the worker thread) when a bitmap landed.
     */
    fun request(
        strip: StillStrip,
        targetWidthPx: Int,
    ) {
        val url = strip.url
        val have = cache.get(url)
        synchronized(lock) {
            if (url in inFlight || pool.isShutdown) return
            val failed = failedAt[url]
            if (failed != null && System.nanoTime() - failed < RETRY_NS) return
            // Fine enough already: the cached shrink is no coarser than the one the screen wants now.
            val fullWidth = have?.let { it.bitmap.width * it.sample } ?: 0
            if (have != null && StillStrips.sampleSize(fullWidth, targetWidthPx) >= have.wanted) return
            inFlight += url
        }
        try {
            pool.execute {
                val ok = runCatching { load(strip, targetWidthPx) }.getOrDefault(false)
                synchronized(lock) {
                    inFlight -= url
                    if (ok) failedAt -= url else failedAt[url] = System.nanoTime()
                }
            }
        } catch (_: RejectedExecutionException) {
            synchronized(lock) { inFlight -= url }
        }
    }

    /**
     * [frame]'s current still uses exactly [urls]: bitmaps of strips nobody uses any more are dropped, and files nobody
     * uses are deleted in the background. A strip the new still shares with the old one stays.
     */
    fun retain(
        frame: String,
        urls: Collection<String>,
    ) {
        for (gone in files.retain(frame, urls)) cache.remove(gone)
        if (pool.isShutdown) return
        try {
            pool.execute { files.purge() }
        } catch (_: RejectedExecutionException) {
            // Leaving: the files go with the cache directory sooner or later.
        }
    }

    /** Frees the cache and the workers (the layer is going away). */
    fun release() {
        pool.shutdownNow()
        cache.evictAll()
    }

    private fun load(
        strip: StillStrip,
        targetWidthPx: Int,
    ): Boolean {
        val file = fileOf(strip) ?: return false
        val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
        BitmapFactory.decodeFile(file.path, bounds)
        if (bounds.outWidth <= 0 || bounds.outHeight <= 0) {
            file.delete()
            return false
        }
        val wanted = StillStrips.sampleSize(bounds.outWidth, targetWidthPx)
        var sample = wanted
        // Keep every bitmap within what the GPU takes as a texture.
        while (max(bounds.outWidth, bounds.outHeight) / sample > MAX_SIDE_PX) sample *= 2
        val options =
            BitmapFactory.Options().apply {
                inSampleSize = sample
                inPreferredConfig = Bitmap.Config.RGB_565
            }
        val bitmap = BitmapFactory.decodeFile(file.path, options)
        if (bitmap != null) {
            cache.put(strip.url, Entry(bitmap, sample, wanted))
            onLoaded()
        }
        return bitmap != null
    }

    /** The strip's file, downloaded first when it is not on disk; null when the server has nothing. */
    private fun fileOf(strip: StillStrip): File? {
        if (!files.has(strip.url)) {
            val bytes = client()?.strip(strip.url)
            if (bytes == null || !files.write(strip.url, bytes)) return null
        }
        return files.file(strip.url)
    }

    private companion object {
        const val DEFAULT_BYTES = 96 * 1024 * 1024
        const val DECODE_THREADS = 2
        const val MAX_SIDE_PX = 4096
        const val RETRY_NS = 3_000_000_000L
    }
}
