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
     * Adds to [out] the page-space areas that must be cleared and redrawn to turn what [previous] (the frame on the
     * buffer, or null for a clean buffer) left there into this frame: by default everything either covers.
     */
    fun changedInto(
        previous: FrontPaint?,
        out: DirtyRects,
    ) {
        val box = RectF()
        boundsInto(box)
        if (previous != null) {
            val before = RectF()
            previous.boundsInto(before)
            if (!before.isEmpty) box.union(before)
        }
        out.add(box.left, box.top, box.right, box.bottom)
    }

    fun draw(
        canvas: Canvas,
        painter: GeometryPainter,
    )
}

/**
 * The stroke under the pen plus an optional predicted tail (motion prediction), which is never committed. A stroke
 * that is translucent (opacity below 1) or multiplies has no tail, and neither does a stamp stroke: the tail is a
 * second layer that overlaps the stroke tip, which would show as a darker patch riding the pen (and, for stamps, be
 * drawn from the one reused raster bitmap); the live geometry already reaches the pen.
 *
 * Live multiply is approximate: the front buffer is transparent, so multiplying against it reduces to plain alpha
 * compositing of the stroke color. The highlighter therefore looks like a tinted overlay until pen-up, when the
 * committed layer multiplies against the page for real.
 */
internal class StrokeFrame(
    private val geometry: InkGeometry,
    predicted: InkGeometry?,
) : FrontPaint {
    private val tail = predicted.takeUnless { !hasTail(geometry) }

    /** True when a predicted tail is drawn after the stroke. */
    val drawsTail: Boolean get() = tail != null
    private val tailBounds = RectF()
    private val changeBox = FloatArray(4)
    private val union = RectF()

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
        out: DirtyRects,
    ) {
        val before = previous as? StrokeFrame
        if (before == null) {
            super.changedInto(previous, out)
            return
        }
        union.setEmpty()
        if (GeometryChange.between(before.geometry, geometry, changeBox)) {
            union.set(
                changeBox[GeometryChange.LEFT],
                changeBox[GeometryChange.TOP],
                changeBox[GeometryChange.RIGHT],
                changeBox[GeometryChange.BOTTOM],
            )
        }
        for (tailGeometry in arrayOf(before.tail, tail)) {
            tailGeometry?.boundsInto(tailBounds)
            if (tailGeometry != null && !tailBounds.isEmpty) union.union(tailBounds)
        }
        out.add(union.left, union.top, union.right, union.bottom)
    }

    override fun draw(
        canvas: Canvas,
        painter: GeometryPainter,
    ) {
        painter.draw(canvas, geometry)
        tail?.let { painter.draw(canvas, it) }
    }

    companion object {
        /** Does a stroke of [geometry] get a predicted tail? Only a solid, normal-blend outline stroke does. */
        fun hasTail(geometry: InkGeometry): Boolean = !geometry.isStamps && geometry.opacity >= 1f && !geometry.multiply
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

    /** A rectangle drag moves two edges only: those are cleared, not the whole box. */
    override fun changedInto(
        previous: FrontPaint?,
        out: DirtyRects,
    ) {
        val before = previous as? ShapeGuideFrame
        if (before == null || !sameRectDrag(before)) {
            super.changedInto(previous, out)
            return
        }
        RectGuideChange.between(x0, y0, before.x1, before.y1, x1, y1, widthPx + ARROW_PAD, out)
    }

    /** Is this a rectangle drag from the same corner with the same line as [other] (only the far corner moved)? */
    private fun sameRectDrag(other: ShapeGuideFrame): Boolean =
        mode == ShapeMode.Rect && other.mode == mode && other.widthPx == widthPx && other.x0 == x0 && other.y0 == y0

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
    private val points = points.copyOf(count * 2)
    private val changeBox = FloatArray(4)
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

    /** The loop only grows: the part from the old last vertex on is cleared and redrawn, not the whole loop box. */
    override fun changedInto(
        previous: FrontPaint?,
        out: DirtyRects,
    ) {
        val before = previous as? LassoFrame
        if (before == null) {
            super.changedInto(previous, out)
            return
        }
        if (!GeometryChange.polyline(before.points, points, changeBox)) return
        val pad = LINE_PX * 2
        out.add(
            changeBox[GeometryChange.LEFT] - pad,
            changeBox[GeometryChange.TOP] - pad,
            changeBox[GeometryChange.RIGHT] + pad,
            changeBox[GeometryChange.BOTTOM] + pad,
        )
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
