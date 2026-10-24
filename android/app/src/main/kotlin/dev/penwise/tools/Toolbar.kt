package dev.penwise.tools

import androidx.compose.foundation.background
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import dev.penwise.R
import dev.penwise.core.InkPresets
import dev.penwise.ink.ShapeMode

private const val PANEL_COLOR = 0xF2FFFFFF
private const val PANEL_RADIUS_DP = 14
private const val PANEL_PAD_DP = 8
private const val GAP_DP = 6
private const val SECTION_GAP_DP = 16

/** Compact floating toolbar: brushes, colors, size, undo and clear. */
@Composable
fun Toolbar(
    state: ToolState,
    onUndo: () -> Unit,
    onClear: () -> Unit,
    onSync: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Column(
        modifier
            .background(Color(PANEL_COLOR), RoundedCornerShape(PANEL_RADIUS_DP.dp))
            .padding(PANEL_PAD_DP.dp),
        verticalArrangement = Arrangement.spacedBy(GAP_DP.dp),
    ) {
        BrushRow(state)
        Row(
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(SECTION_GAP_DP.dp),
        ) {
            ColorRow(state)
            SizeControl(state)
            ToolChip(stringResource(R.string.tool_eraser), selected = state.eraserMode, onClick = state::toggleEraser)
            ShapeChips(state)
            ToolChip(stringResource(R.string.tool_undo), selected = false, onClick = onUndo)
            ToolChip(stringResource(R.string.tool_clear_page), selected = false, onClick = onClear)
            ToolChip(stringResource(R.string.tool_sync), selected = false, onClick = onSync)
        }
    }
}

@Composable
private fun BrushRow(state: ToolState) {
    Row(
        Modifier.horizontalScroll(rememberScrollState()),
        horizontalArrangement = Arrangement.spacedBy(GAP_DP.dp),
    ) {
        InkPresets.names.forEachIndexed { index, name ->
            ToolChip(name, selected = index == state.presetIndex, onClick = { state.selectPreset(index) })
        }
    }
}

@Composable
private fun ShapeChips(state: ToolState) {
    val labels =
        listOf(
            ShapeMode.Line to R.string.shape_line,
            ShapeMode.Rect to R.string.shape_rect,
            ShapeMode.Ellipse to R.string.shape_ellipse,
            ShapeMode.Arrow to R.string.shape_arrow,
        )
    labels.forEach { (mode, label) ->
        ToolChip(
            stringResource(label),
            selected = state.shapeMode == mode,
            onClick = { state.toggleShape(mode) },
        )
    }
}

@Composable
private fun ColorRow(state: ToolState) {
    Row(
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(GAP_DP.dp),
    ) {
        ToolChip(
            stringResource(R.string.tool_preset_color),
            selected = state.swatchRgb == null,
            onClick = { state.selectSwatch(null) },
        )
        ColorSwatch(state.presetArgb(), selected = state.swatchRgb == null, onClick = { state.selectSwatch(null) })
        Palette.swatches.forEach { rgb ->
            ColorSwatch(rgb, selected = state.swatchRgb == rgb, onClick = { state.selectSwatch(rgb) })
        }
    }
}

@Composable
private fun SizeControl(state: ToolState) {
    Row(
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(GAP_DP.dp),
    ) {
        SizeSlider(
            value = state.sizeScale,
            range = ToolState.SIZE_MIN..ToolState.SIZE_MAX,
            onChange = state::changeSize,
        )
        BasicText(stringResource(R.string.tool_size_value, state.sizeScale))
    }
}
