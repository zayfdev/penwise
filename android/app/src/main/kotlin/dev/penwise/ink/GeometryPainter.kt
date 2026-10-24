package dev.penwise.ink

import android.graphics.BlendMode
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import android.graphics.Path
import android.graphics.RectF
import androidx.core.graphics.withTranslation
import dev.penwise.core.InkGeometry

/** Draws [InkGeometry] on a Canvas. Not thread-safe: one instance per render layer. */
internal class GeometryPainter(
    private val grain: GrainMasks,
) {
    private val path = Path()
    private val bounds = RectF()
    private val shaders = DabShaders()
    private val fillPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply { style = Paint.Style.FILL }
    private val dabPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = Color.BLACK }
    private val layerPaint = Paint()

    fun draw(
        canvas: Canvas,
        geometry: InkGeometry,
    ) {
        if (geometry.isStamps) drawStamps(canvas, geometry) else drawOutline(canvas, geometry)
    }

    private fun drawOutline(
        canvas: Canvas,
        g: InkGeometry,
    ) {
        path.rewind()
        path.fillType = Path.FillType.WINDING
        val points = g.outline.size / 2
        var start = 0
        for (rawEnd in g.contourEnds) {
            val end = rawEnd.coerceAtMost(points)
            if (end - start > 2) {
                path.moveTo(g.outline[2 * start], g.outline[2 * start + 1])
                for (p in start + 1 until end) path.lineTo(g.outline[2 * p], g.outline[2 * p + 1])
                path.close()
            }
            start = end
        }
        fillPaint.color = g.argb
        fillPaint.alpha = g.strokeAlpha()
        fillPaint.blendMode = if (g.multiply) BlendMode.MULTIPLY else null
        canvas.drawPath(path, fillPaint)
    }

    /** Dabs accumulate inside one layer, so overlaps never exceed the stroke opacity applied at restore. */
    private fun drawStamps(
        canvas: Canvas,
        g: InkGeometry,
    ) {
        g.boundsInto(bounds)
        if (bounds.isEmpty) return
        layerPaint.alpha = g.strokeAlpha()
        layerPaint.blendMode = if (g.multiply) BlendMode.MULTIPLY else null
        val save = canvas.saveLayer(bounds, layerPaint)
        val d = g.dabs
        var i = 0
        while (i + Dab.STRIDE <= d.size) {
            drawDab(canvas, d, i)
            i += Dab.STRIDE
        }
        grain.paint(d[Dab.GRAIN_STRENGTH], d[Dab.GRAIN_LEVEL])?.let { canvas.drawRect(bounds, it) }
        canvas.drawColor(g.argb or OPAQUE, BlendMode.SRC_IN)
        canvas.restoreToCount(save)
    }

    private fun drawDab(
        canvas: Canvas,
        d: FloatArray,
        o: Int,
    ) {
        val rx = d[o + Dab.RADIUS_X]
        val ry = d[o + Dab.RADIUS_Y]
        if (rx <= 0f || ry <= 0f) return
        dabPaint.shader = shaders.forHardness(d[o + Dab.HARDNESS])
        dabPaint.alpha = alpha255(d[o + Dab.OPACITY])
        canvas.withTranslation(d[o + Dab.X], d[o + Dab.Y]) {
            rotate(Math.toDegrees(d[o + Dab.ANGLE].toDouble()).toFloat())
            scale(rx, ry)
            drawCircle(0f, 0f, 1f, dabPaint)
        }
    }

    private companion object {
        const val OPAQUE = -0x1000000
    }
}
