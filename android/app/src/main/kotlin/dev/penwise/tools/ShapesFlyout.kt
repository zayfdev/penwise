package dev.penwise.tools

import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.ColorFilter
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import dev.penwise.R
import dev.penwise.frames.ChromeStyle
import dev.penwise.ink.ShapeMode

private const val COLUMNS = 2
private const val TILE_HEIGHT_DP = 52
private const val PANEL_WIDTH_DP = 196
private const val TITLE_SP = 13

private class ShapeEntry(
    val mode: ShapeMode,
    val icon: ImageVector,
    val label: Int,
)

private val SHAPES =
    listOf(
        ShapeEntry(ShapeMode.Line, DockIcons.Line, R.string.shape_line),
        ShapeEntry(ShapeMode.Arrow, DockIcons.Arrow, R.string.shape_arrow),
        ShapeEntry(ShapeMode.Rect, DockIcons.Rect, R.string.shape_rect),
        ShapeEntry(ShapeMode.Ellipse, DockIcons.Ellipse, R.string.shape_ellipse),
    )

/** The shapes dropdown: line, arrow, rectangle, ellipse. Picking one selects it (or clears it) and closes it. */
@Composable
internal fun ShapesFlyout(
    state: ToolState,
    onPicked: () -> Unit,
) {
    val colors = ChromeStyle.colors
    FlyoutPanel(PANEL_WIDTH_DP.dp, gap = 12.dp) {
        DockText(stringResource(R.string.drawer_shapes), colors.text, TITLE_SP, weight = FontWeight.SemiBold)
        TileGrid(SHAPES, COLUMNS) { entry ->
            val selected = state.shapeMode == entry.mode
            val tint by animateColorAsState(
                if (selected) colors.iconOn else colors.icon,
                tween(DockMotion.SELECT_MS, easing = DockMotion.easeOut),
                label = "shapeTint",
            )
            FlyoutTile(
                TileSpec(stringResource(entry.label), selected, TILE_HEIGHT_DP.dp),
                onClick = {
                    state.toggleShape(entry.mode)
                    onPicked()
                },
                modifier = Modifier.weight(1f),
            ) {
                Image(
                    imageVector = entry.icon,
                    contentDescription = null,
                    colorFilter = ColorFilter.tint(tint),
                    modifier = Modifier.size(entry.icon.defaultWidth),
                )
            }
        }
    }
}
