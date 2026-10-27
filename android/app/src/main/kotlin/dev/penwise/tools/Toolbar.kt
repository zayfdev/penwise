package dev.penwise.tools

import androidx.compose.foundation.background
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
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
private const val SECTION_GAP_DP = 14
private const val RGB_MASK = 0xFFFFFF

/** Floating toolbar: brush previews, colours, size, shapes, eraser, undo and page actions — all as icons. */
@Composable
fun Toolbar(
    state: ToolState,
    onUndo: () -> Unit,
    onRedo: () -> Unit,
    onClear: () -> Unit,
    onSync: () -> Unit,
    onHome: () -> Unit,
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
            ToolGroup {
                GlyphChip(
                    glyph = Glyph.Eraser,
                    label = stringResource(R.string.tool_eraser),
                    selected = state.eraserMode,
                    onClick = state::toggleEraser,
                )
                if (state.eraserMode) {
                    GlyphChip(
                        glyph = Glyph.PreciseEraser,
                        label = stringResource(R.string.tool_precise),
                        selected = state.preciseErase,
                        onClick = state::togglePreciseErase,
                    )
                }
            }
            ShapeChips(state)
            LassoChips(state)
            ToolGroup {
                GlyphChip(
                    glyph = Glyph.Undo,
                    label = stringResource(R.string.tool_undo),
                    selected = false,
                    onClick = onUndo,
                )
                GlyphChip(
                    glyph = Glyph.Redo,
                    label = stringResource(R.string.tool_redo),
                    selected = false,
                    onClick = onRedo,
                )
                GlyphChip(
                    glyph = Glyph.Trash,
                    label = stringResource(R.string.tool_clear_page),
                    selected = false,
                    onClick = onClear,
                )
            }
            ToolGroup {
                GlyphChip(
                    glyph = Glyph.Sync,
                    label = stringResource(R.string.tool_sync),
                    selected = false,
                    onClick = onSync,
                )
                GlyphChip(
                    glyph = Glyph.Home,
                    label = stringResource(R.string.tool_home),
                    selected = false,
                    onClick = onHome,
                )
            }
        }
    }
}

/** One horizontally scrollable row of brush previews; the preview is drawn by the brush itself. */
@Composable
private fun BrushRow(state: ToolState) {
    Row(
        Modifier.horizontalScroll(rememberScrollState()),
        horizontalArrangement = Arrangement.spacedBy(GAP_DP.dp),
    ) {
        InkPresets.names.forEachIndexed { index, name ->
            BrushIcon(
                preset = index,
                argb = state.presetArgb(index),
                label = name,
                selected = index == state.presetIndex,
                onClick = { state.selectPreset(index) },
            )
        }
    }
}

@Composable
private fun ShapeChips(state: ToolState) {
    val shapes =
        listOf(
            Triple(ShapeMode.Line, Glyph.Line, R.string.shape_line),
            Triple(ShapeMode.Rect, Glyph.Rect, R.string.shape_rect),
            Triple(ShapeMode.Ellipse, Glyph.Ellipse, R.string.shape_ellipse),
            Triple(ShapeMode.Arrow, Glyph.Arrow, R.string.shape_arrow),
        )
    ToolGroup {
        shapes.forEach { (mode, glyph, label) ->
            GlyphChip(
                glyph = glyph,
                label = stringResource(label),
                selected = state.shapeMode == mode,
                onClick = { state.toggleShape(mode) },
            )
        }
    }
}

@Composable
private fun LassoChips(state: ToolState) {
    ToolGroup {
        GlyphChip(
            glyph = Glyph.Lasso,
            label = stringResource(R.string.tool_lasso),
            selected = state.lassoMode,
            onClick = state::toggleLasso,
        )
        if (state.selection.count > 0) {
            GlyphChip(
                glyph = Glyph.Trash,
                label = stringResource(R.string.tool_delete_selection),
                selected = false,
                onClick = state.selection::requestDelete,
            )
        }
    }
}

@Composable
private fun ToolGroup(content: @Composable () -> Unit) {
    Row(
        horizontalArrangement = Arrangement.spacedBy(2.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        content()
    }
}

@Composable
private fun ColorRow(state: ToolState) {
    Row(
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(GAP_DP.dp),
    ) {
        ColorSwatch(
            state.presetArgb(),
            selected = state.swatchRgb == null,
            onClick = { state.selectSwatch(null) },
            label = stringResource(R.string.tool_preset_color),
        )
        Palette.swatches.forEach { rgb ->
            ColorSwatch(
                rgb,
                selected = state.swatchRgb == rgb,
                onClick = { state.selectSwatch(rgb) },
                label = rgb.toHexColor(),
            )
        }
    }
}

/** Screen-reader name for a swatch. */
private fun Int.toHexColor(): String = "#%06X".format(this and RGB_MASK)

@Composable
private fun SizeControl(state: ToolState) {
    SizeSlider(
        value = state.sizeScale,
        range = ToolState.SIZE_MIN..ToolState.SIZE_MAX,
        label = stringResource(R.string.tool_size_value, state.sizeScale),
        onChange = state::changeSize,
    )
}
