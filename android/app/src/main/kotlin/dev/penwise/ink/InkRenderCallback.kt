package dev.penwise.ink

import android.graphics.BlendMode
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.RectF
import androidx.core.graphics.withClip
import androidx.graphics.lowlatency.CanvasFrontBufferedRenderer

/**
 * Front layer: the newest snapshot of the in-progress stroke (clearing only the area the previous snapshot covered).
 * Multi-buffered layer: white page, multiply strokes first (under-ink band), then normal strokes. Runs on the
 * renderer thread; the param is only a wake-up token, data comes from [LiveStroke] and [StrokeStore].
 */
internal class InkRenderCallback(
    private val store: StrokeStore,
    private val live: LiveStroke,
    private val style: () -> PageStyle,
) : CanvasFrontBufferedRenderer.Callback<Unit> {
    private val grain = GrainMasks()
    private val frontPainter = GeometryPainter(grain)
    private val pagePainter = GeometryPainter(grain)
    private val dirty = RectF()
    private val next = RectF()
    private var drawnStrokeId = NO_STROKE

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
        frame.geometry.boundsInto(next)
        dirty.union(next)
        canvas.withClip(dirty) {
            drawColor(Color.TRANSPARENT, BlendMode.CLEAR)
            frontPainter.draw(this, frame.geometry)
        }
        dirty.set(next)
    }

    override fun onDrawMultiBufferedLayer(
        canvas: Canvas,
        bufferWidth: Int,
        bufferHeight: Int,
        params: Collection<Unit>,
    ) {
        PaperPainter.draw(canvas, style(), bufferWidth, bufferHeight)
        val strokes = store.strokes
        for (g in strokes) if (g.multiply) pagePainter.draw(canvas, g)
        for (g in strokes) if (!g.multiply) pagePainter.draw(canvas, g)
    }

    private companion object {
        const val NO_STROKE = -1L
    }
}
