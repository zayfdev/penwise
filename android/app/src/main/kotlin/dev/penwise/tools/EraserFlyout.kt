package dev.penwise.tools

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.runtime.Composable
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import dev.penwise.R

private const val PANEL_WIDTH_DP = 280

/** The eraser flyout: its size (the pen's slider) and whether it cuts strokes or removes them whole. */
@Composable
internal fun EraserFlyout(state: ToolState) {
    FlyoutPanel(PANEL_WIDTH_DP.dp) {
        FlyoutHeader(stringResource(R.string.tool_eraser), stringResource(R.string.tool_size_value, state.sizeScale))
        SizeControl(state)
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            FlyoutToggle(stringResource(R.string.pen_precise_erase), state.preciseErase, state::togglePreciseErase)
        }
    }
}
