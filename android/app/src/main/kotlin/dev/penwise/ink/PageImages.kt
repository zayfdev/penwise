package dev.penwise.ink

import android.graphics.Bitmap
import android.util.Log
import android.util.LruCache
import org.json.JSONObject
import java.util.concurrent.Executors

/** One picture placed on a page: an asset id and the page-pixel rectangle it covers. */
data class PageImage(
    val id: String,
    val asset: String,
    val x: Float,
    val y: Float,
    val w: Float,
    val h: Float,
    val label: String?,
) {
    companion object {
        /** Reads the wire form; `null` when the picture has no usable rectangle. */
        fun read(json: JSONObject): PageImage? {
            val asset = json.optString("asset").takeIf { it.isNotEmpty() } ?: return null
            val w = json.optDouble("w", 0.0).toFloat()
            val h = json.optDouble("h", 0.0).toFloat()
            if (!(w > 0f) || !(h > 0f)) return null
            return PageImage(
                id = json.optString("id"),
                asset = asset,
                x = json.optDouble("x", 0.0).toFloat(),
                y = json.optDouble("y", 0.0).toFloat(),
                w = w,
                h = h,
                label = json.optString("label").takeIf { it.isNotEmpty() },
            )
        }
    }
}

/**
 * The pictures on the page on screen, with their decoded bitmaps.
 *
 * Pictures are decoration for the drawing surface: they are not part of the stroke store or its undo history,
 * because the server (or an agent) placed them. Decoding happens on one background thread; the render thread
 * only ever reads a bitmap that is already decoded, and repaints when a new one arrives.
 */
class PageImages(
    private val fetch: (String) -> Bitmap?,
) {
    private val pictures = LinkedHashMap<String, PageImage>()
    private val cache =
        object : LruCache<String, Bitmap>(MAX_BYTES) {
            override fun sizeOf(
                key: String,
                value: Bitmap,
            ): Int = value.byteCount
        }
    private val loading = HashSet<String>()
    private val worker =
        Executors.newSingleThreadExecutor { runnable ->
            Thread(runnable, "penwise-images").apply { isDaemon = true }
        }

    /** Fires after a picture decodes, so the surface can repaint. */
    @Volatile
    var onChanged: (() -> Unit)? = null

    /** Replaces every picture (a page snapshot). */
    fun replace(list: List<PageImage>) {
        val changed =
            synchronized(pictures) {
                val same =
                    pictures.size == list.size &&
                        list.all { pictures[it.id] == it }
                if (!same) {
                    pictures.clear()
                    list.forEach { pictures[it.id] = it }
                }
                !same
            }
        if (changed) loadMissing()
    }

    /** Adds or updates one picture (an `image` frame). */
    fun add(image: PageImage) {
        synchronized(pictures) { pictures[image.id] = image }
        loadMissing()
    }

    /** Forgets everything; used when another page is opened. */
    fun clear() {
        synchronized(pictures) { pictures.clear() }
        cache.evictAll()
    }

    /** The pictures in placement order; safe to call from the render thread. */
    fun all(): List<PageImage> = synchronized(pictures) { pictures.values.toList() }

    /** The decoded bitmap, if it has finished loading. */
    fun bitmap(asset: String): Bitmap? = cache.get(asset)

    /** Decodes whatever is not in memory yet, one at a time. */
    private fun loadMissing() {
        for (image in all()) {
            if (cache.get(image.asset) == null && claim(image.asset)) decode(image.asset)
        }
    }

    /** True when this call is the one that will load `asset`. */
    private fun claim(asset: String): Boolean = synchronized(loading) { loading.add(asset) }

    private fun decode(asset: String) {
        worker.execute {
            val bitmap = runCatching { fetch(asset) }.getOrNull()
            if (bitmap != null) cache.put(asset, bitmap) else Log.w(TAG, "picture $asset did not load")
            synchronized(loading) { loading.remove(asset) }
            if (bitmap != null) onChanged?.invoke()
        }
    }

    private companion object {
        const val TAG = "PenwiseImages"
        const val MAX_BYTES = 48 * 1024 * 1024
    }
}
