package dev.penwise.ink

import android.graphics.BlendMode
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Matrix
import android.graphics.Paint
import androidx.core.graphics.withMatrix
import kotlin.math.ceil
import kotlin.math.max
import kotlin.math.min

/**
 * Draws the paper under the ink: white sheet plus the page's ruling, seen through the view transform.
 *
 * Colours, spacing, line width and dot size match the server's PNG renderer and the browser (`render/paper.rs`,
 * `paper.ts`): rules sit at the positive multiples of the spacing in page coordinates and dots are 3 px squares. A
 * fixed sheet is surrounded by [DESK_COLOR]; an infinite one is paper everywhere and its ruling continues in every
 * direction. Only the part under the canvas clip is drawn. Rules stay at least one screen pixel wide when zoomed out.
 */
internal object PaperPainter {
    private const val RULED_COLOR = 0xFFC9D6E8.toInt()
    private const val GRID_COLOR = 0xFFDCE3EC.toInt()
    private const val DOTS_COLOR = 0xFFC6CEDA.toInt()
    private const val DESK_COLOR = 0xFFE4E7EC.toInt()
    private const val LINE_WIDTH_PX = 1f
    private const val DOT_HALF_PX = 1.5f
    private const val EDGE_PAD_PX = 2f
    private const val SPACING = PageStyle.SPACING_PX

    private val line =
        Paint().apply {
            style = Paint.Style.FILL
            isAntiAlias = false
        }
    private val dot =
        Paint().apply {
            style = Paint.Style.FILL
            isAntiAlias = true
        }
    private val matrix = Matrix()

    /** What shows where the page is not (yet) drawn: the desk around a fixed sheet, paper for an infinite one. */
    fun backdrop(style: PageStyle): Int = if (style.infinite) Color.WHITE else DESK_COLOR

    /** Paints the sheet through [view]; a page without a pattern is simply white. */
    fun draw(
        canvas: Canvas,
        style: PageStyle,
        view: ViewTransform,
    ) {
        canvas.drawColor(backdrop(style), BlendMode.SRC)
        val clip = canvas.clipBounds
        var left = view.toPageX(clip.left.toFloat()) - EDGE_PAD_PX
        var top = view.toPageY(clip.top.toFloat()) - EDGE_PAD_PX
        var right = view.toPageX(clip.right.toFloat()) + EDGE_PAD_PX
        var bottom = view.toPageY(clip.bottom.toFloat()) + EDGE_PAD_PX
        matrix.setScale(view.scale, view.scale)
        matrix.postTranslate(view.tx, view.ty)
        canvas.withMatrix(matrix) {
            if (!style.infinite) {
                left = max(left, 0f)
                top = max(top, 0f)
                right = min(right, style.widthPx)
                bottom = min(bottom, style.heightPx)
                if (right <= left || bottom <= top) return@withMatrix
                line.color = Color.WHITE
                drawRect(left, top, right, bottom, line)
            }
            pattern(this, style, left, top, right, bottom, max(LINE_WIDTH_PX, 1f / view.scale))
        }
    }

    private fun pattern(
        canvas: Canvas,
        style: PageStyle,
        left: Float,
        top: Float,
        right: Float,
        bottom: Float,
        lineWidth: Float,
    ) {
        // Rules sit on the positive multiples of the spacing; an infinite sheet has them on both sides.
        val first = if (style.infinite) Int.MIN_VALUE else 1
        val cols = ruleRange(left, right, first)
        val rows = ruleRange(top, bottom, first)
        when (style.background) {
            "ruled" -> {
                line.color = RULED_COLOR
                for (r in rows) canvas.drawRect(left, r * SPACING, right, r * SPACING + lineWidth, line)
            }

            "grid" -> {
                line.color = GRID_COLOR
                for (c in cols) canvas.drawRect(c * SPACING, top, c * SPACING + lineWidth, bottom, line)
                for (r in rows) canvas.drawRect(left, r * SPACING, right, r * SPACING + lineWidth, line)
            }

            "dots" -> {
                dot.color = DOTS_COLOR
                for (r in rows) {
                    val y = r * SPACING
                    for (c in cols) {
                        val x = c * SPACING
                        canvas.drawRect(x - DOT_HALF_PX, y - DOT_HALF_PX, x + DOT_HALF_PX, y + DOT_HALF_PX, dot)
                    }
                }
            }
        }
    }

    /** Indexes `k >= first` of the multiples `k * spacing` that fall inside `[from, to)`. */
    private fun ruleRange(
        from: Float,
        to: Float,
        first: Int,
    ): IntRange = max(first, ceil(from / SPACING).toInt())..(ceil(to / SPACING).toInt() - 1)
}
