package dev.penwise.ink

import android.graphics.Canvas
import android.graphics.Matrix
import android.graphics.Paint
import android.graphics.Path
import android.graphics.RectF
import dev.penwise.core.InkGeometry

/**
 * Something drawn on the front buffer while a gesture runs: made on the UI thread, drawn on the render thread. It is
 * described in page space; the canvas is in screen space and `painter.view` maps one to the other.
 */
internal interface FrontPaint {
    /** Writes the page-space area this frame covers into [out] (empty when nothing is drawn). */
    fun boundsInto(out: RectF)

    /**
     * Writes the page-space area that must be cleared and redrawn to turn what [previous] (the frame on the buffer,
     * or null for a clean buffer) left there into this frame: by default everything either covers.
     */
    fun changedInto(
        previous: FrontPaint?,
        out: RectF,
    ) {
        boundsInto(out)
        if (previous == null) return
        val before = RectF()
        previous.boundsInto(before)
        if (!before.isEmpty) out.union(before)
    }

    fun draw(
        canvas: Canvas,
        painter: GeometryPainter,
    )
}

/**
 * The stroke under the pen plus an optional predicted tail (motion prediction), which is never committed. A stamp
 * stroke has no tail: its dabs are one layer (a second layer would double the stroke opacity where they overlap, and
 * both would be drawn from the one reused raster bitmap), and the live geometry already reaches the pen.
 */
internal class StrokeFrame(
    private val geometry: InkGeometry,
    predicted: InkGeometry?,
) : FrontPaint {
    private val tail = predicted.takeUnless { geometry.isStamps }

    /** True when a predicted tail is drawn after the stroke. */
    val drawsTail: Boolean get() = tail != null
    private val tailBounds = RectF()
    private val changeBox = FloatArray(4)

    override fun boundsInto(out: RectF) {
        geometry.boundsInto(out)
        tail?.let {
            it.boundsInto(tailBounds)
            out.union(tailBounds)
        }
    }

    /** Only the stroke's changed part and the old and new tails: the rest is already on the buffer. */
    override fun changedInto(
        previous: FrontPaint?,
        out: RectF,
    ) {
        val before = previous as? StrokeFrame
        if (before == null) {
            super.changedInto(previous, out)
            return
        }
        out.setEmpty()
        if (GeometryChange.between(before.geometry, geometry, changeBox)) {
            out.set(
                changeBox[GeometryChange.LEFT],
                changeBox[GeometryChange.TOP],
                changeBox[GeometryChange.RIGHT],
                changeBox[GeometryChange.BOTTOM],
            )
        }
        for (tailGeometry in arrayOf(before.tail, tail)) {
            tailGeometry?.boundsInto(tailBounds)
            if (tailGeometry != null && !tailBounds.isEmpty) out.union(tailBounds)
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
        }

    override fun boundsInto(out: RectF) {
        out.set(minOf(x0, x1), minOf(y0, y1), maxOf(x0, x1), maxOf(y0, y1))
        out.inset(-(widthPx + ARROW_PAD), -(widthPx + ARROW_PAD))
    }

    override fun draw(
        canvas: Canvas,
        painter: GeometryPainter,
    ) {
        val view = painter.view
        val sx0 = view.toScreenX(x0)
        val sy0 = view.toScreenY(y0)
        val sx1 = view.toScreenX(x1)
        val sy1 = view.toScreenY(y1)
        val (left, right) = minOf(sx0, sx1) to maxOf(sx0, sx1)
        val (top, bottom) = minOf(sy0, sy1) to maxOf(sy0, sy1)
        paint.strokeWidth = widthPx * view.scale
        when (mode) {
            ShapeMode.Line -> canvas.drawLine(sx0, sy0, sx1, sy1, paint)
            ShapeMode.Arrow -> canvas.drawLine(sx0, sy0, sx1, sy1, paint)
            ShapeMode.Rect -> canvas.drawRect(left, top, right, bottom, paint)
            ShapeMode.Ellipse -> canvas.drawOval(left, top, right, bottom, paint)
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
    private val screenPath = Path()
    private val matrix = Matrix()
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
        val view = painter.view
        matrix.setScale(view.scale, view.scale)
        matrix.postTranslate(view.tx, view.ty)
        path.transform(matrix, screenPath)
        canvas.drawPath(screenPath, paint)
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
        val view = painter.view
        canvas.drawRect(
            view.toScreenX(box.left + dx),
            view.toScreenY(box.top + dy),
            view.toScreenX(box.right + dx),
            view.toScreenY(box.bottom + dy),
            paint,
        )
    }

    private companion object {
        const val PAD = 6f
    }
}
