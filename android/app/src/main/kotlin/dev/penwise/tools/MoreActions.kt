package dev.penwise.tools

import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.ColorFilter
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import dev.penwise.R
import dev.penwise.frames.ChromeStyle

private const val COLUMNS = 3
private const val TILE_HEIGHT_DP = 64
private const val LABEL_SP = 11
private const val PANEL_WIDTH_DP = 360

/** What the page actions flyout can do; the screen owns each of them. */
internal class MoreCommands(
    val onClear: () -> Unit,
    val onSync: () -> Unit,
    val onHome: () -> Unit,
    val onAddFrame: () -> Unit,
    val onFit: () -> Unit,
    val onShowDone: () -> Unit,
)

private class Action(
    val icon: ImageVector,
    val label: Int,
    val run: () -> Unit,
)

/** The page actions flyout: clear the page, sync, home, a new frame, fit the page and show the done marks. */
@Composable
internal fun MoreActions(
    commands: MoreCommands,
    zoomPercent: Int,
    showDone: Boolean,
) {
    val colors = ChromeStyle.colors
    val actions =
        listOf(
            Action(DockIcons.ClearPage, R.string.tool_clear_page, commands.onClear),
            Action(DockIcons.Sync, R.string.tool_sync, commands.onSync),
            Action(DockIcons.Home, R.string.tool_home, commands.onHome),
            Action(DockIcons.NewFrame, R.string.tool_add_frame, commands.onAddFrame),
            Action(DockIcons.Fit, R.string.tool_fit_page, commands.onFit),
        )
    FlyoutPanel(PANEL_WIDTH_DP.dp, gap = 12.dp) {
        FlyoutHeader(stringResource(R.string.tool_more_actions), stringResource(R.string.view_percent, zoomPercent))
        TileGrid(actions, COLUMNS) { action ->
            val label = stringResource(action.label)
            FlyoutTile(
                TileSpec(label, false, TILE_HEIGHT_DP.dp),
                onClick = action.run,
                modifier = Modifier.weight(1f),
            ) {
                Image(
                    action.icon,
                    null,
                    Modifier.size(action.icon.defaultWidth),
                    colorFilter = ColorFilter.tint(colors.icon),
                )
                DockText(label, colors.icon, LABEL_SP)
            }
        }
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            FlyoutToggle(stringResource(R.string.marks_show_done), showDone, commands.onShowDone)
        }
    }
}
