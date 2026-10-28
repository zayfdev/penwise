package dev.penwise.ink

import android.graphics.Bitmap
import android.graphics.BlendMode
import android.graphics.Canvas
import android.graphics.Color
import android.util.Log
import androidx.core.graphics.createBitmap
import java.util.concurrent.Executors
import java.util.concurrent.atomic.AtomicReference

/** Everything one off-thread page render needs: a frozen copy of the scene's decoded items and the target view. */
internal class SettleRequest(
    val view: ViewTransform,
    val style: PageStyle,
    val width: Int,
    val height: Int,
    val order: List<ItemKey>,
    val items: Map<ItemKey, ItemRender>,
    /** Draw the paper, or the ink alone on a transparent bitmap. */
    val paper: Boolean,
)

/** A finished render: [bitmap] shows the page under [view] as of the snapshot it was requested with. */
internal class SettleResult(
    val bitmap: Bitmap,
    val view: ViewTransform,
)

/**
 * Renders the visible page into a spare bitmap on a background thread once a pan/zoom has settled, so the render
 * thread only ever draws one transformed bitmap per frame. Newer requests supersede older ones and stale results are
 * recycled, never published. It has its own [GeometryPainter]; the snapshot in the request is immutable, so nothing
 * is shared with the render thread but the (thread-safe) [PaperPainter].
 */
internal class SettleRenderer(
    images: () -> PageImages?,
    onReady: () -> Unit,
) {
    private val painter = GeometryPainter(images)
    private val spare = AtomicReference<Bitmap?>()
    private val executor =
        Executors.newSingleThreadExecutor { task ->
            Thread(task, "penwise-settle").apply {
                isDaemon = true
                priority = Thread.NORM_PRIORITY - 2
            }
        }
    private val latest = LatestRenderer(executor, ::render, { recycle(it.bitmap) }, onReady, ::failed)
    private var reported = false

    val pending: Boolean get() = latest.pending

    fun request(request: SettleRequest) = latest.submit(request)

    fun cancel() = latest.cancel()

    fun take(): SettleResult? = latest.take()

    /** Keeps [bitmap] as the next render target (or frees it when a spare is already kept). */
    fun recycle(bitmap: Bitmap) {
        spare.getAndSet(bitmap)?.recycle()
    }

    fun release() {
        executor.shutdownNow()
        latest.cancel()
        spare.getAndSet(null)?.recycle()
    }

    private fun render(
        request: SettleRequest,
        current: () -> Boolean,
    ): SettleResult? {
        val bitmap = acquire(request.width, request.height)
        val surface = Canvas(bitmap)
        val view = request.view
        painter.view = view
        if (request.paper) {
            PaperPainter.draw(surface, request.style, view)
        } else {
            surface.drawColor(Color.TRANSPARENT, BlendMode.CLEAR)
        }
        val visible =
            Bounds(
                view.toPageX(0f),
                view.toPageY(0f),
                view.toPageX(request.width.toFloat()),
                view.toPageY(request.height.toFloat()),
            )
        for (key in request.order) {
            if (!current()) {
                recycle(bitmap)
                return null
            }
            val item = request.items[key] ?: continue
            if (item.bounds.intersects(visible)) painter.draw(surface, item)
        }
        return SettleResult(bitmap, view)
    }

    /** A render that threw (out of memory, malformed item): logged once; the page keeps showing the scaled bitmap. */
    private fun failed(error: Throwable) {
        if (reported) return
        reported = true
        Log.e(TAG, "settle render failed", error)
    }

    /** A bitmap of this size to render into: the kept spare or a new one. Safe from any thread. */
    fun acquire(
        width: Int,
        height: Int,
    ): Bitmap {
        val kept = spare.getAndSet(null)
        if (kept != null && kept.width == width && kept.height == height) return kept
        kept?.recycle()
        return createBitmap(width, height, Bitmap.Config.ARGB_8888)
    }

    private companion object {
        const val TAG = "PenwiseSettle"
    }
}
