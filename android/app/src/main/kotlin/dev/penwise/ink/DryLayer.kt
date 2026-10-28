package dev.penwise.ink

import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Matrix
import android.graphics.Paint
import android.graphics.RectF
import android.os.SystemClock
import androidx.core.graphics.createBitmap
import androidx.core.graphics.withClip
import androidx.core.graphics.withMatrix
import java.util.concurrent.atomic.AtomicBoolean

/**
 * The committed page as one persistent screen-sized bitmap (paper, pictures, ink) rendered through the view
 * transform: a commit draws only the new item into it, an erase or undo repaints only the area of the items that
 * changed, and the multi-buffered layer is a single `drawBitmap`. While a pan/zoom gesture runs the bitmap is simply
 * drawn transformed (cheap; re-rendered now and then so the edges fill in), and when the gesture settles the visible
 * page is rendered again at the new scale, so ink is crisp when zoomed in. Render thread only; [invalidate] is the
 * one entry point for other threads.
 */
internal class DryLayer(
    private val painter: GeometryPainter,
) {
    private var bitmap: Bitmap? = null
    private var canvas: Canvas? = null
    private var rendered: ViewTransform? = null
    private var pendingFull = false
    private var liveRepaintAt = NOT_ARMED
    private val clip = RectF()
    private val visible = Bounds()
    private val region = Bounds()
    private val matrix = Matrix()
    private val bitmapPaint = Paint(Paint.FILTER_BITMAP_FLAG)
    private val stale = AtomicBoolean(true)

    /** Marks the whole layer for repaint (paper style, picture arrival, page switch). */
    fun invalidate() {
        stale.set(true)
    }

    /**
     * Brings the bitmap in line with the scene, the paper and the [view], then draws it onto [target]. While
     * [gesturing] a bitmap rendered under another view is shown transformed instead of rendered again.
     */
    fun drawInto(
        target: Canvas,
        cache: SceneRenderCache?,
        style: PageStyle,
        view: ViewTransform,
        gesturing: Boolean,
    ) {
        val resized = ensureBitmap(target.width, target.height)
        val surface = canvas ?: return
        // One atomic step: an invalidate() landing after this read keeps the flag set for the next frame.
        val full = stale.getAndSet(false) || resized || pendingFull
        val plan = cache?.update() ?: RenderPlan.Idle
        val shown = rendered
        val moved = shown != null && shown != view
        if (moved && gesturing && postponeRepaint()) {
            pendingFull = full || plan !is RenderPlan.Idle
            drawScaled(target, style, requireNotNull(shown), view)
            return
        }
        val started = SystemClock.uptimeMillis()
        paint(surface, cache, style, view, plan, full || moved || shown == null)
        pendingFull = false
        rendered = view
        val end = SystemClock.uptimeMillis()
        // A repaint while the fingers are still down is rationed by what it costs, so the gesture never stalls.
        liveRepaintAt = if (gesturing) end + maxOf(LIVE_REPAINT_MS, LIVE_REPAINT_COST * (end - started)) else NOT_ARMED
        bitmap?.let { target.drawBitmap(it, 0f, 0f, null) }
    }

    /** Creates the bitmap for a new size; true when it is new (and so blank). */
    private fun ensureBitmap(
        width: Int,
        height: Int,
    ): Boolean {
        val resized = bitmap?.width != width || bitmap?.height != height
        if (resized) {
            bitmap?.recycle()
            bitmap = createBitmap(width, height, Bitmap.Config.ARGB_8888)
            canvas = Canvas(requireNotNull(bitmap))
        }
        return resized
    }

    /** During a gesture, true while the last render is recent enough to be shown transformed instead. */
    private fun postponeRepaint(): Boolean {
        val now = SystemClock.uptimeMillis()
        if (liveRepaintAt == NOT_ARMED) liveRepaintAt = now + LIVE_REPAINT_MS
        return now < liveRepaintAt
    }

    private fun paint(
        surface: Canvas,
        cache: SceneRenderCache?,
        style: PageStyle,
        view: ViewTransform,
        plan: RenderPlan,
        everything: Boolean,
    ) {
        painter.view = view
        when {
            everything || plan is RenderPlan.Full -> paintAll(surface, cache, style, view)
            plan is RenderPlan.Append -> plan.items.forEach { painter.draw(surface, it) }
            plan is RenderPlan.Region -> paintRegion(surface, cache, style, view, plan.area)
        }
    }

    /** Draws the bitmap rendered under [from] as it looks under [to]; what it does not cover shows the backdrop. */
    private fun drawScaled(
        target: Canvas,
        style: PageStyle,
        from: ViewTransform,
        to: ViewTransform,
    ) {
        target.drawColor(PaperPainter.backdrop(style))
        val k = to.scale / from.scale
        matrix.setScale(k, k)
        matrix.postTranslate(to.tx - k * from.tx, to.ty - k * from.ty)
        bitmap?.let { target.withMatrix(matrix) { drawBitmap(it, 0f, 0f, bitmapPaint) } }
    }

    private fun paintAll(
        surface: Canvas,
        cache: SceneRenderCache?,
        style: PageStyle,
        view: ViewTransform,
    ) {
        PaperPainter.draw(surface, style, view)
        cache ?: return
        visible.left = view.toPageX(0f)
        visible.top = view.toPageY(0f)
        visible.right = view.toPageX(surface.width.toFloat())
        visible.bottom = view.toPageY(surface.height.toFloat())
        for (key in cache.paintOrder) {
            val item = cache[key] ?: continue
            if (item.bounds.intersects(visible)) painter.draw(surface, item)
        }
    }

    private fun paintRegion(
        surface: Canvas,
        cache: SceneRenderCache?,
        style: PageStyle,
        view: ViewTransform,
        area: Bounds,
    ) {
        cache ?: return
        clip.set(
            view.toScreenX(area.left) - CLIP_PAD_PX,
            view.toScreenY(area.top) - CLIP_PAD_PX,
            view.toScreenX(area.right) + CLIP_PAD_PX,
            view.toScreenY(area.bottom) + CLIP_PAD_PX,
        )
        region.left = area.left
        region.top = area.top
        region.right = area.right
        region.bottom = area.bottom
        region.inset(CLIP_PAD_PX / view.scale)
        surface.withClip(clip) {
            PaperPainter.draw(this, style, view)
            for (key in cache.paintOrder) {
                val item = cache[key] ?: continue
                if (item.bounds.intersects(region)) painter.draw(this, item)
            }
        }
    }

    private companion object {
        const val NOT_ARMED = 0L
        const val LIVE_REPAINT_MS = 150L
        const val LIVE_REPAINT_COST = 3L
        const val CLIP_PAD_PX = 2f
    }
}
