package dev.penwise.ink

import android.graphics.BlendMode
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.RectF
import androidx.core.graphics.withClip

/**
 * Paints the committed page into a bitmap canvas: all of it, one region, or items appended on top. Render thread.
 * Without [paper] the layer is ink only on a transparent background, so a live page can show through the surface
 * ([dev.penwise.frames.FrameLayer]); the paper is then drawn by an ordinary view below the frames.
 */
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
        paper: Boolean = true,
    ) {
        painter.view = view
        when {
            everything || plan is RenderPlan.Full -> paintAll(surface, cache, style, view, paper)
            plan is RenderPlan.Append -> plan.items.forEach { painter.draw(surface, it) }
            plan is RenderPlan.Region -> paintRegion(surface, cache, style, view, plan.area, paper)
        }
    }

    private fun paintAll(
        surface: Canvas,
        cache: SceneRenderCache?,
        style: PageStyle,
        view: ViewTransform,
        paper: Boolean,
    ) {
        backdrop(surface, style, view, paper)
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
        paper: Boolean,
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
            backdrop(this, style, view, paper)
            for (key in cache.paintOrder) {
                val item = cache[key] ?: continue
                if (item.bounds.intersects(region)) painter.draw(this, item)
            }
        }
    }

    /** The paper (and the desk around a fixed sheet), or nothing at all: the pixels an erase or a repaint vacates. */
    private fun backdrop(
        surface: Canvas,
        style: PageStyle,
        view: ViewTransform,
        paper: Boolean,
    ) {
        if (paper) PaperPainter.draw(surface, style, view) else surface.drawColor(Color.TRANSPARENT, BlendMode.CLEAR)
    }

    private companion object {
        const val CLIP_PAD_PX = 2f
    }
}
