package dev.penwise.ink

import android.graphics.BlendMode
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.RectF
import androidx.core.graphics.withClip
import androidx.graphics.lowlatency.CanvasFrontBufferedRenderer

/**
 * Front layer: the newest frame of the gesture in flight (clearing only the area the previous frame covered).
 * Multi-buffered layer: one `drawBitmap` of the [DryLayer], which holds the paper and the committed items and
 * repaints only what the scene reported dirty. Runs on the renderer thread; the param is only a wake-up token.
 */
internal class InkRenderCallback(
    private val live: LiveStroke,
    private val style: () -> PageStyle,
    images: () -> PageImages? = { null },
) : CanvasFrontBufferedRenderer.Callback<Unit> {
    private val grain = GrainMasks()
    private val frontPainter = GeometryPainter(grain, images)
    private val dry = DryLayer(GeometryPainter(grain, images))
    private val dirty = RectF()
    private val next = RectF()
    private var drawnStrokeId = NO_STROKE

    @Volatile
    private var cache: SceneRenderCache? = null

    /** Shows [scene] (or nothing); the next multi-buffered draw repaints the page from it. */
    fun attach(scene: PageScene?) {
        cache = scene?.let { SceneRenderCache(it) }
        dry.invalidate()
    }

    /** Repaints the whole page on the next multi-buffered draw. */
    fun invalidate() = dry.invalidate()

    override fun onDrawFrontBufferedLayer(
        canvas: Canvas,
        bufferWidth: Int,
        bufferHeight: Int,
        param: Unit,
    ) {
        val frame = live.take() ?: return
        if (frame.strokeId != drawnStrokeId) {
            canvas.drawColor(Color.TRANSPARENT, BlendMode.CLEAR)
            drawnStrokeId = frame.strokeId
            dirty.setEmpty()
        }
        frame.paint.boundsInto(next)
        dirty.union(next)
        canvas.withClip(dirty) {
            drawColor(Color.TRANSPARENT, BlendMode.CLEAR)
            frame.paint.draw(this, frontPainter)
        }
        dirty.set(next)
    }

    override fun onDrawMultiBufferedLayer(
        canvas: Canvas,
        bufferWidth: Int,
        bufferHeight: Int,
        params: Collection<Unit>,
    ) {
        dry.drawInto(canvas, cache, style())
    }

    private companion object {
        const val NO_STROKE = -1L
    }
}
