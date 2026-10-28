package dev.penwise.ink

import android.graphics.Canvas
import android.graphics.RectF
import androidx.core.graphics.withClip

/** Paints the committed page into a bitmap canvas: all of it, one region, or items appended on top. Render thread. */
internal class PagePainter(
    private val painter: GeometryPainter,
) {
    private val clip = RectF()
    private val visible = Bounds()
    private val region = Bounds()

    fun paint(
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
        const val CLIP_PAD_PX = 2f
    }
}
