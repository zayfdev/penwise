package dev.penwise.ink

import android.graphics.BlendMode
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.RectF
import androidx.core.graphics.withClip
import androidx.graphics.lowlatency.CanvasFrontBufferedRenderer

/**
 * Front layer: the newest frame of the gesture in flight, drawn through the [viewport] transform (clearing only the
 * screen area the previous frame covered). Multi-buffered layer: one `drawBitmap` of the [DryLayer], which holds the
 * paper and the committed items, repaints only what the scene reported dirty and re-renders off-thread after the view
 * changed. Runs on the
 * renderer thread; the param is only a wake-up token.
 */
internal class InkRenderCallback(
    private val live: LiveStroke,
    private val viewport: ViewportState,
    private val style: () -> PageStyle,
    images: () -> PageImages? = { null },
    onSettled: () -> Unit = {},
) : CanvasFrontBufferedRenderer.Callback<Unit> {
    private val frontPainter = GeometryPainter(images)
    private val dry = DryLayer(GeometryPainter(images), images, onSettled)
    private val dirty = RectF()
    private val next = RectF()
    private var drawnPaint: FrontPaint? = null
    private var drawnView: ViewTransform? = null
    private var drawnStrokeId = NO_STROKE

    @Volatile
    private var cache: SceneRenderCache? = null

    /** Shows [scene] (or nothing); the next multi-buffered draw repaints the page from it. */
    fun attach(scene: PageScene?) {
        cache = scene?.let { SceneRenderCache(it) }
        dry.reset()
    }

    /** Stops the background render thread; the callback is not used afterwards. */
    fun release() = dry.release()

    /** Repaints the whole page on the next multi-buffered draw. */
    fun invalidate() = dry.invalidate()

    override fun onDrawFrontBufferedLayer(
        canvas: Canvas,
        bufferWidth: Int,
        bufferHeight: Int,
        param: Unit,
    ) {
        val frame = live.take() ?: return
        val view = viewport.transform
        if (frame.strokeId != drawnStrokeId || view != drawnView) {
            canvas.drawColor(Color.TRANSPARENT, BlendMode.CLEAR)
            drawnStrokeId = frame.strokeId
            drawnView = view
            drawnPaint = null
        }
        frontPainter.view = view
        // Only what changed since the frame on the buffer is cleared and redrawn (see GeometryChange): clearing
        // and repainting the whole stroke every frame shows up as flashing lines on the single-buffered layer.
        frame.paint.changedInto(drawnPaint, next)
        drawnPaint = frame.paint
        if (next.isEmpty) return
        screenBounds(view, next, dirty)
        canvas.withClip(dirty) {
            drawColor(Color.TRANSPARENT, BlendMode.CLEAR)
            frame.paint.draw(this, frontPainter)
        }
    }

    /** The screen rect of a page-space [page] rect, with room for anti-aliasing and round caps. */
    private fun screenBounds(
        view: ViewTransform,
        page: RectF,
        out: RectF,
    ) {
        out.set(
            view.toScreenX(page.left) - EDGE_PX,
            view.toScreenY(page.top) - EDGE_PX,
            view.toScreenX(page.right) + EDGE_PX,
            view.toScreenY(page.bottom) + EDGE_PX,
        )
    }

    override fun onDrawMultiBufferedLayer(
        canvas: Canvas,
        bufferWidth: Int,
        bufferHeight: Int,
        params: Collection<Unit>,
    ) {
        dry.drawInto(canvas, cache, style(), viewport.transform, viewport.gesturing)
    }

    private companion object {
        const val NO_STROKE = -1L
        const val EDGE_PX = 8f
    }
}
