package dev.penwise.overview

import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawWithCache
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp

/** The overview's look: a dark canvas with a dotted grid, like a design canvas. */
internal object OverviewLook {
    val canvas = Color(0xFF0E1014)
    val dot = Color(0xFF262B34)
    val cardFill = Color(0xFF1B1F27)
    val paper = Color(0xFFF1EFE9)
    val paperText = Color(0xFF23252A)
    const val DOT_SPACING_DP = 28
    const val DOT_RADIUS_DP = 1.4f
}

/** Paints the dotted canvas behind the strips. */
internal fun Modifier.dottedCanvas(): Modifier =
    drawWithCache {
        val spacing = OverviewLook.DOT_SPACING_DP.dp.toPx()
        val radius = OverviewLook.DOT_RADIUS_DP.dp.toPx()
        onDrawBehind {
            drawRect(OverviewLook.canvas)
            var y = spacing / 2f
            while (y < size.height) {
                var x = spacing / 2f
                while (x < size.width) {
                    drawCircle(OverviewLook.dot, radius, Offset(x, y))
                    x += spacing
                }
                y += spacing
            }
        }
    }
