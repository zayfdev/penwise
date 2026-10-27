package dev.penwise.ink

import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.RectF
import androidx.core.graphics.createBitmap
import androidx.core.graphics.withClip
import java.util.concurrent.atomic.AtomicBoolean

/**
 * The committed page as one persistent bitmap (paper, pictures, ink): a commit draws only the new item into it, an
 * erase or undo repaints only the area of the items that changed, and the multi-buffered layer is a single
 * `drawBitmap`. Render thread only; [invalidate] is the one entry point for other threads.
 */
internal class DryLayer(
    private val painter: GeometryPainter,
) {
    private var bitmap: Bitmap? = null
    private var canvas: Canvas? = null
    private val clip = RectF()
    private val stale = AtomicBoolean(true)

    /** Marks the whole layer for repaint (paper style, picture arrival, page switch). */
    fun invalidate() {
        stale.set(true)
    }

    /** Brings the bitmap in line with the scene and the paper, then draws it onto [target]. */
    fun drawInto(
        target: Canvas,
        cache: SceneRenderCache?,
        style: PageStyle,
    ) {
        val width = target.width
        val height = target.height
        val resized = bitmap?.width != width || bitmap?.height != height
        if (resized) {
            bitmap?.recycle()
            bitmap = createBitmap(width, height, Bitmap.Config.ARGB_8888)
            canvas = Canvas(requireNotNull(bitmap))
        }
        val surface = canvas ?: return
        // One atomic step: an invalidate() landing after this read keeps the flag set for the next frame.
        val full = stale.getAndSet(false) || resized
        val plan = cache?.update() ?: RenderPlan.Idle
        when {
            full || plan is RenderPlan.Full -> paintAll(surface, cache, style)
            plan is RenderPlan.Append -> plan.items.forEach { painter.draw(surface, it) }
            plan is RenderPlan.Region -> paintRegion(surface, cache, style, plan.area)
        }
        bitmap?.let { target.drawBitmap(it, 0f, 0f, null) }
    }

    private fun paintAll(
        surface: Canvas,
        cache: SceneRenderCache?,
        style: PageStyle,
    ) {
        PaperPainter.draw(surface, style, surface.width, surface.height)
        cache ?: return
        for (key in cache.paintOrder) cache[key]?.let { painter.draw(surface, it) }
    }

    private fun paintRegion(
        surface: Canvas,
        cache: SceneRenderCache?,
        style: PageStyle,
        area: Bounds,
    ) {
        cache ?: return
        clip.set(area.left, area.top, area.right, area.bottom)
        surface.withClip(clip) {
            PaperPainter.draw(this, style, width, height)
            for (key in cache.paintOrder) {
                val item = cache[key] ?: continue
                if (item.bounds.intersects(area)) painter.draw(this, item)
            }
        }
    }
}
