package dev.penwise.tools

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import dev.penwise.R
import dev.penwise.frames.ChromeStyle

private const val SIZE_SP = 12

/** The size row of a flyout: the caption, the value in monospace and the slider (the pen and the eraser share it). */
@Composable
internal fun SizeControl(state: ToolState) {
    val colors = ChromeStyle.colors
    val size = stringResource(R.string.tool_size_value, state.sizeScale)
    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
            DockText(stringResource(R.string.pen_size), colors.textMuted, SIZE_SP)
            DockText(size, colors.textMuted, SIZE_SP, family = ChromeStyle.mono)
        }
        SizeSlider(
            value = state.sizeScale,
            range = ToolState.SIZE_MIN..ToolState.SIZE_MAX,
            label = size,
            onChange = state::changeSize,
        )
    }
}
