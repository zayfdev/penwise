package dev.penwise.ink

import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.Path
import android.graphics.RectF
import dev.penwise.core.InkGeometry

/** Something drawn on the front buffer while a gesture runs: made on the UI thread, drawn on the render thread. */
internal interface FrontPaint {
    /** Writes the page-space area this frame covers into [out] (empty when nothing is drawn). */
    fun boundsInto(out: RectF)

    fun draw(
        canvas: Canvas,
        painter: GeometryPainter,
    )
}

/** The stroke under the pen plus an optional predicted tail (motion prediction), which is never committed. */
internal class StrokeFrame(
    private val geometry: InkGeometry,
    private val tail: InkGeometry?,
) : FrontPaint {
    private val tailBounds = RectF()

    override fun boundsInto(out: RectF) {
        geometry.boundsInto(out)
        tail?.let {
            it.boundsInto(tailBounds)
            out.union(tailBounds)
        }
    }

    override fun draw(
        canvas: Canvas,
        painter: GeometryPainter,
    ) {
        painter.draw(canvas, geometry)
        tail?.let { painter.draw(canvas, it) }
    }
}

/** Guide for a shape drag: the drag box as the shape's outline. The committed shape is built by the core. */
internal class ShapeGuideFrame(
    private val mode: ShapeMode,
    private val x0: Float,
    private val y0: Float,
    private val x1: Float,
    private val y1: Float,
    argb: Int,
    private val widthPx: Float,
) : FrontPaint {
    private val paint =
        Paint(Paint.ANTI_ALIAS_FLAG).apply {
            style = Paint.Style.STROKE
            strokeCap = Paint.Cap.ROUND
            strokeJoin = Paint.Join.ROUND
            color = argb
            alpha = GUIDE_ALPHA
            strokeWidth = widthPx
        }

    override fun boundsInto(out: RectF) {
        out.set(minOf(x0, x1), minOf(y0, y1), maxOf(x0, x1), maxOf(y0, y1))
        out.inset(-(widthPx + ARROW_PAD), -(widthPx + ARROW_PAD))
    }

    override fun draw(
        canvas: Canvas,
        painter: GeometryPainter,
    ) {
        when (mode) {
            ShapeMode.Line -> canvas.drawLine(x0, y0, x1, y1, paint)
            ShapeMode.Arrow -> canvas.drawLine(x0, y0, x1, y1, paint)
            ShapeMode.Rect -> canvas.drawRect(minOf(x0, x1), minOf(y0, y1), maxOf(x0, x1), maxOf(y0, y1), paint)
            ShapeMode.Ellipse -> canvas.drawOval(minOf(x0, x1), minOf(y0, y1), maxOf(x0, x1), maxOf(y0, y1), paint)
        }
    }

    private companion object {
        const val GUIDE_ALPHA = 200
        const val ARROW_PAD = 4f
    }
}

/** The lasso loop being drawn: a dashed polyline from the pen-down point. */
internal class LassoFrame(
    points: FloatArray,
    count: Int,
) : FrontPaint {
    private val path = Path()
    private val box = RectF()
    private val paint =
        Paint(Paint.ANTI_ALIAS_FLAG).apply {
            style = Paint.Style.STROKE
            color = SELECTION_COLOR
            strokeWidth = LINE_PX
            pathEffect = android.graphics.DashPathEffect(floatArrayOf(DASH_PX, DASH_PX), 0f)
        }

    init {
        if (count > 0) {
            path.moveTo(points[0], points[1])
            for (i in 1 until count) path.lineTo(points[2 * i], points[2 * i + 1])
            path.computeBounds(box, true)
        }
    }

    override fun boundsInto(out: RectF) {
        out.set(box)
        if (!out.isEmpty) out.inset(-LINE_PX * 2, -LINE_PX * 2)
    }

    override fun draw(
        canvas: Canvas,
        painter: GeometryPainter,
    ) {
        canvas.drawPath(path, paint)
    }

    companion object {
        const val SELECTION_COLOR = 0xFF1A73E8.toInt()
        const val LINE_PX = 3f
        const val DASH_PX = 14f
    }
}

/** Frame around the selected items, shifted by the drag so far. */
internal class SelectionFrame(
    private val box: RectF,
    private val dx: Float,
    private val dy: Float,
) : FrontPaint {
    private val paint =
        Paint(Paint.ANTI_ALIAS_FLAG).apply {
            style = Paint.Style.STROKE
            color = LassoFrame.SELECTION_COLOR
            strokeWidth = LassoFrame.LINE_PX
        }

    override fun boundsInto(out: RectF) {
        out.set(box)
        out.offset(dx, dy)
        out.inset(-PAD, -PAD)
    }

    override fun draw(
        canvas: Canvas,
        painter: GeometryPainter,
    ) {
        canvas.drawRect(box.left + dx, box.top + dy, box.right + dx, box.bottom + dy, paint)
    }

    private companion object {
        const val PAD = 6f
    }
}
