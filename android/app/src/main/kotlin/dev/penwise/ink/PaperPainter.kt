package dev.penwise.ink

import android.graphics.BlendMode
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint

/**
 * Draws the paper under the ink: white sheet plus the page's ruling, on the tablet.
 *
 * Colours, spacing, line width and dot size match the server's PNG renderer and the browser (`render/paper.rs`,
 * `paper.ts`): rules sit at the positive multiples of the spacing in page coordinates and dots are 3 px squares. The
 * clients skip patterns under 4 px apart; the tablet draws 1:1 at 93.7 px, so that never applies here. Without this
 * the tablet is a blank screen while the PC shows lines and dots.
 */
internal object PaperPainter {
    private const val RULED_COLOR = 0xFFC9D6E8.toInt()
    private const val GRID_COLOR = 0xFFDCE3EC.toInt()
    private const val DOTS_COLOR = 0xFFC6CEDA.toInt()
    private const val LINE_WIDTH_PX = 1f
    private const val DOT_HALF_PX = 1.5f

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

    /** Paints the sheet across the canvas; a page without a pattern is simply white. */
    fun draw(
        canvas: Canvas,
        style: PageStyle,
        canvasWidth: Int,
        canvasHeight: Int,
    ) {
        canvas.drawColor(Color.WHITE, BlendMode.SRC)
        val width = if (style.widthPx > 0f) style.widthPx else canvasWidth.toFloat()
        val height = if (style.heightPx > 0f) style.heightPx else canvasHeight.toFloat()
        when (style.background) {
            "ruled" -> {
                line.color = RULED_COLOR
                var y = PageStyle.SPACING_PX
                while (y < height) {
                    canvas.drawRect(0f, y, width, y + LINE_WIDTH_PX, line)
                    y += PageStyle.SPACING_PX
                }
            }

            "grid" -> {
                line.color = GRID_COLOR
                var x = PageStyle.SPACING_PX
                while (x < width) {
                    canvas.drawRect(x, 0f, x + LINE_WIDTH_PX, height, line)
                    x += PageStyle.SPACING_PX
                }
                var y = PageStyle.SPACING_PX
                while (y < height) {
                    canvas.drawRect(0f, y, width, y + LINE_WIDTH_PX, line)
                    y += PageStyle.SPACING_PX
                }
            }

            "dots" -> {
                dot.color = DOTS_COLOR
                var y = PageStyle.SPACING_PX
                while (y < height) {
                    var x = PageStyle.SPACING_PX
                    while (x < width) {
                        canvas.drawRect(x - DOT_HALF_PX, y - DOT_HALF_PX, x + DOT_HALF_PX, y + DOT_HALF_PX, dot)
                        x += PageStyle.SPACING_PX
                    }
                    y += PageStyle.SPACING_PX
                }
            }
        }
    }
}
