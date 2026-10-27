// Icon geometry lives in a 24×24 viewport: these numbers are the drawing, not tunable knobs.
@file:Suppress("MagicNumber")

package dev.penwise.tools

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.PathEffect
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.rotate
import androidx.compose.ui.graphics.drawscope.scale
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp

/** Every tool icon: drawn as paths so the app carries no icon dependency. */
enum class Glyph {
    Eraser,
    PreciseEraser,
    Undo,
    Redo,
    Trash,
    Sync,
    Home,
    Plus,
    Line,
    Rect,
    Ellipse,
    Arrow,
    Lasso,
    Pencil,
    Notebook,
    Close,
}

private const val GLYPH_TINT = 0xFF23252A.toInt()
private const val SELECTED_TINT = 0xFF2F4FD0.toInt()
private const val CHIP_BACKGROUND = 0x14000000
private const val SELECTED_BACKGROUND = 0x1A2F4FD0
private const val CHIP_PADDING_DP = 7
private const val GLYPH_SIZE_DP = 20
private const val STROKE_WIDTH = 1.8f
private const val VIEWPORT = 24f

/** A tappable icon with an optional selected state; the label is for screen readers only. */
@Composable
fun GlyphChip(
    glyph: Glyph,
    label: String,
    selected: Boolean,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Box(
        modifier =
            modifier
                .background(
                    if (selected) Color(SELECTED_BACKGROUND) else Color(CHIP_BACKGROUND),
                    RoundedCornerShape(8.dp),
                ).clickable(onClick = onClick)
                .padding(CHIP_PADDING_DP.dp)
                .semantics { contentDescription = label },
        contentAlignment = Alignment.Center,
    ) {
        GlyphCanvas(
            glyph = glyph,
            tint = Color(if (selected) SELECTED_TINT else GLYPH_TINT),
            modifier = Modifier.size(GLYPH_SIZE_DP.dp),
        )
    }
}

/** The icon on its own, for rows that draw their own background. */
@Composable
fun GlyphCanvas(
    glyph: Glyph,
    tint: Color,
    modifier: Modifier = Modifier,
) {
    Canvas(modifier) {
        val factor = size.minDimension / VIEWPORT
        scale(factor, factor, pivot = Offset.Zero) {
            GLYPH_PAINTERS[glyph]?.invoke(this, tint)
        }
    }
}

/** One painter per icon, all in a 24×24 coordinate space. */
private val GLYPH_PAINTERS: Map<Glyph, DrawScope.(Color) -> Unit> =
    mapOf(
        Glyph.Eraser to { tint -> eraser(tint, precise = false) },
        Glyph.PreciseEraser to { tint -> eraser(tint, precise = true) },
        Glyph.Undo to { tint -> undoLike(tint, forward = false) },
        Glyph.Redo to { tint -> undoLike(tint, forward = true) },
        Glyph.Trash to { tint -> trash(tint) },
        Glyph.Sync to { tint -> sync(tint) },
        Glyph.Home to { tint -> home(tint) },
        Glyph.Plus to { tint -> gLine(tint, 12f, 6f, 12f, 18f).also { gLine(tint, 6f, 12f, 18f, 12f) } },
        Glyph.Line to { tint -> gLine(tint, 5f, 19f, 19f, 5f) },
        Glyph.Rect to { tint -> gBox(tint, 5f, 6f, 19f, 18f) },
        Glyph.Ellipse to { tint ->
            drawOval(
                color = tint,
                topLeft = Offset(4f, 7f),
                size = Size(16f, 11f),
                style = strokeStyle(),
            )
        },
        Glyph.Arrow to { tint -> arrow(tint) },
        Glyph.Lasso to { tint ->
            drawOval(
                color = tint,
                topLeft = Offset(4f, 5f),
                size = Size(16f, 12f),
                style = strokeStyle(PathEffect.dashPathEffect(floatArrayOf(3f, 3f))),
            )
            gLine(tint, 12f, 17f, 14f, 20f)
        },
        Glyph.Pencil to { tint -> pencil(tint) },
        Glyph.Notebook to { tint -> notebook(tint) },
        Glyph.Close to { tint -> gLine(tint, 7f, 7f, 17f, 17f).also { gLine(tint, 17f, 7f, 7f, 17f) } },
    )

private fun strokeStyle(effect: PathEffect? = null) =
    Stroke(width = STROKE_WIDTH, cap = StrokeCap.Round, pathEffect = effect)

private fun DrawScope.gLine(
    tint: Color,
    x1: Float,
    y1: Float,
    x2: Float,
    y2: Float,
) {
    drawLine(tint, Offset(x1, y1), Offset(x2, y2), strokeWidth = STROKE_WIDTH, cap = StrokeCap.Round)
}

private fun DrawScope.gBox(
    tint: Color,
    left: Float,
    top: Float,
    right: Float,
    bottom: Float,
    effect: PathEffect? = null,
) {
    drawRect(
        color = tint,
        topLeft = Offset(left, top),
        size = Size(right - left, bottom - top),
        style = strokeStyle(effect),
    )
}

/** Eraser block, dashed when it cuts instead of deleting whole strokes. */
private fun DrawScope.eraser(
    tint: Color,
    precise: Boolean,
) {
    rotate(degrees = -35f, pivot = Offset(12f, 12f)) {
        val effect = if (precise) PathEffect.dashPathEffect(floatArrayOf(2.2f, 1.8f)) else null
        gBox(tint, 7f, 8f, 17f, 16f, effect)
        if (!precise) gLine(tint, 7f, 12f, 17f, 12f)
    }
    gLine(tint, 4f, 20f, 20f, 20f)
}

/** Circular arrow with its head; [forward] mirrors the direction. */
private fun DrawScope.undoLike(
    tint: Color,
    forward: Boolean,
) {
    val headX = if (forward) 15f else 9f
    val tipX = if (forward) 19f else 5f
    val path =
        Path().apply {
            moveTo(headX, 6f)
            lineTo(tipX, 10f)
            lineTo(headX, 14f)
        }
    drawPath(path, tint, style = strokeStyle())
    drawArc(
        color = tint,
        startAngle = if (forward) -170f else 150f,
        sweepAngle = 250f,
        useCenter = false,
        topLeft = Offset(if (forward) 5f else 6f, 6f),
        size = Size(13f, 13f),
        style = strokeStyle(),
    )
}

private fun DrawScope.trash(tint: Color) {
    gLine(tint, 5f, 7f, 19f, 7f)
    gLine(tint, 10f, 5f, 14f, 5f)
    gBox(tint, 7f, 9f, 17f, 20f)
    gLine(tint, 10f, 12f, 10f, 17f)
    gLine(tint, 14f, 12f, 14f, 17f)
}

private fun DrawScope.sync(tint: Color) {
    drawArc(tint, 200f, 140f, false, Offset(4f, 6f), Size(12f, 12f), style = strokeStyle())
    drawArc(tint, 20f, 140f, false, Offset(8f, 6f), Size(12f, 12f), style = strokeStyle())
    gLine(tint, 15f, 4.5f, 17.5f, 7f)
    gLine(tint, 9f, 17f, 6.5f, 19.5f)
}

private fun DrawScope.home(tint: Color) {
    val roof =
        Path().apply {
            moveTo(4f, 11f)
            lineTo(12f, 4f)
            lineTo(20f, 11f)
        }
    drawPath(roof, tint, style = strokeStyle())
    gLine(tint, 6f, 11f, 6f, 20f)
    gLine(tint, 18f, 11f, 18f, 20f)
    gLine(tint, 6f, 20f, 18f, 20f)
}

private fun DrawScope.arrow(tint: Color) {
    gLine(tint, 5f, 19f, 19f, 5f)
    gLine(tint, 19f, 5f, 13f, 6f)
    gLine(tint, 19f, 5f, 18f, 11f)
}

private fun DrawScope.pencil(tint: Color) {
    val body =
        Path().apply {
            moveTo(6f, 18f)
            lineTo(8f, 12f)
            lineTo(16f, 4f)
            lineTo(20f, 8f)
            lineTo(12f, 16f)
            close()
        }
    drawPath(body, tint, style = strokeStyle())
    gLine(tint, 8f, 12f, 12f, 16f)
}

private fun DrawScope.notebook(tint: Color) {
    gBox(tint, 6f, 4f, 18f, 20f)
    gLine(tint, 9f, 4f, 9f, 20f)
    gLine(tint, 12f, 9f, 16f, 9f)
    gLine(tint, 12f, 13f, 16f, 13f)
}
