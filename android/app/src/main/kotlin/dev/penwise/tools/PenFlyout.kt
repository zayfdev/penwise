package dev.penwise.tools

import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import dev.penwise.R
import dev.penwise.core.InkPresets
import dev.penwise.frames.ChromeStyle

private const val COLUMNS = 4
private const val TILE_HEIGHT_DP = 64
private const val PREVIEW_DP = 38
private const val LABEL_SP = 11
private const val RGB_MASK = 0x00FFFFFF
private const val ALPHA_MASK = -0x1000000
private const val PANEL_WIDTH_DP = 360

/** The pen flyout: every pen type with its engine-drawn preview, the size slider and the pen toggles. */
@Composable
internal fun PenFlyout(state: ToolState) {
    val colors = ChromeStyle.colors
    val size = stringResource(R.string.tool_size_value, state.sizeScale)
    FlyoutPanel(PANEL_WIDTH_DP.dp) {
        FlyoutHeader(
            stringResource(R.string.pen_title),
            "${InkPresets.names.getOrElse(state.presetIndex) { "" }} · $size",
        )
        TileGrid(InkPresets.names.indices.toList(), COLUMNS) { index ->
            // The preview is the brush in the chrome's text colour (keeping the preset's alpha), so it reads here.
            val preview = (state.presetArgb(index) and ALPHA_MASK) or (colors.text.toArgb() and RGB_MASK)
            val label = brushLabel(InkPresets.names[index])
            FlyoutTile(
                TileSpec(label, index == state.presetIndex, TILE_HEIGHT_DP.dp),
                onClick = {
                    state.selectPreset(index)
                    state.draw()
                },
                modifier = Modifier.weight(1f),
            ) {
                BrushPreview(index, preview, size = PREVIEW_DP.dp)
                DockText(label, colors.icon, LABEL_SP)
            }
        }
        SizeControl(state)
    }
}

/** `ballpoint` as `Ballpoint`. */
internal fun brushLabel(name: String): String = name.replaceFirstChar { it.uppercase() }
