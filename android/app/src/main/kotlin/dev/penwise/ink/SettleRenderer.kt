package dev.penwise.ink

import android.graphics.Bitmap
import android.graphics.Canvas
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
    /** The scene state the snapshot shows ([DryLayer] compares it with the live one before swapping). */
    val epoch: Long,
) {
    /** Does this request already render [view] / [style] at scene state [epoch]? */
    fun covers(
        view: ViewTransform,
        epoch: Long,
        style: PageStyle,
    ): Boolean = this.view == view && this.epoch == epoch && this.style == style
}

/** A finished render: [bitmap] shows the page under [view] as of scene state [epoch]. */
internal class SettleResult(
    val bitmap: Bitmap,
    val view: ViewTransform,
    val epoch: Long,
) {
    /** Is this the render of exactly this [view], scene state and bitmap size? */
    fun matches(
        view: ViewTransform,
        epoch: Long,
        width: Int,
        height: Int,
    ): Boolean = this.view == view && this.epoch == epoch && bitmap.width == width && bitmap.height == height
}

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
    private val latest = LatestRenderer(executor, ::render, { recycle(it.bitmap) }, onReady)

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
        PaperPainter.draw(surface, request.style, view)
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
        return SettleResult(bitmap, view, request.epoch)
    }

    private fun acquire(
        width: Int,
        height: Int,
    ): Bitmap {
        val kept = spare.getAndSet(null)
        if (kept != null && kept.width == width && kept.height == height) return kept
        kept?.recycle()
        return createBitmap(width, height, Bitmap.Config.ARGB_8888)
    }
}
