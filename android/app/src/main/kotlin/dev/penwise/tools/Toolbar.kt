package dev.penwise.tools

import androidx.compose.foundation.background
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.penwise.R
import dev.penwise.core.InkPresets
import dev.penwise.ink.ShapeMode

private const val PANEL_COLOR = 0xF2FFFFFF
private const val PANEL_RADIUS_DP = 14
private const val PANEL_PAD_DP = 8
private const val GAP_DP = 6
private const val SECTION_GAP_DP = 14
private const val RGB_MASK = 0xFFFFFF
private const val CAPTION_GAP_DP = 3
private const val GROUP_GAP_DP = 4
private const val CAPTION_SP = 11
private const val CAPTION_COLOR = 0xFF6A6D75

/**
 * The drawer's tool panel: brush previews, colours, size, eraser, shapes, lasso, page actions and the view controls,
 * each group under a short text caption. Undo/redo live in the always-visible [MiniBar] instead.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun Toolbar(
    state: ToolState,
    zoomPercent: Int,
    showDone: Boolean,
    onShowDone: () -> Unit,
    onFit: () -> Unit,
    onClear: () -> Unit,
    onSync: () -> Unit,
    onHome: () -> Unit,
    onAddFrame: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Column(
        modifier
            .background(Color(PANEL_COLOR), RoundedCornerShape(PANEL_RADIUS_DP.dp))
            .padding(PANEL_PAD_DP.dp),
        verticalArrangement = Arrangement.spacedBy(GAP_DP.dp),
    ) {
        Section(stringResource(R.string.drawer_brush, InkPresets.names.getOrElse(state.presetIndex) { "" })) {
            BrushRow(state)
        }
        FlowRow(
            horizontalArrangement = Arrangement.spacedBy(SECTION_GAP_DP.dp),
            verticalArrangement = Arrangement.spacedBy(GAP_DP.dp),
        ) {
            Section(stringResource(R.string.drawer_colour)) { ColorRow(state) }
            Section(stringResource(R.string.drawer_size)) { SizeControl(state) }
            Section(stringResource(R.string.drawer_erase)) {
                ToolButton(
                    icon = ToolIcons.Eraser,
                    label = stringResource(R.string.tool_eraser),
                    selected = state.eraserMode,
                    onClick = state::toggleEraser,
                    caption = true,
                )
                if (state.eraserMode) {
                    ToolButton(
                        icon = ToolIcons.PreciseEraser,
                        label = stringResource(R.string.tool_precise),
                        selected = state.preciseErase,
                        onClick = state::togglePreciseErase,
                        caption = true,
                    )
                }
            }
            Section(stringResource(R.string.drawer_shapes)) { ShapeChips(state) }
            Section(stringResource(R.string.drawer_select)) { LassoChips(state) }
            Section(stringResource(R.string.drawer_page)) {
                ToolButton(
                    ToolIcons.ClearPage,
                    stringResource(R.string.tool_clear_page),
                    false,
                    onClear,
                    caption = true,
                )
                ToolButton(ToolIcons.Sync, stringResource(R.string.tool_sync), false, onSync, caption = true)
                ToolButton(ToolIcons.Home, stringResource(R.string.tool_home), false, onHome, caption = true)
                ToolButton(ToolIcons.Frame, stringResource(R.string.tool_add_frame), false, onAddFrame, caption = true)
            }
            Section(stringResource(R.string.drawer_marks)) {
                ToolChip(
                    label = stringResource(R.string.marks_show_done),
                    selected = showDone,
                    onClick = onShowDone,
                )
            }
            Section(stringResource(R.string.drawer_view, zoomPercent)) {
                ToolButton(ToolIcons.Fit, stringResource(R.string.tool_fit_page), false, onFit, caption = true)
            }
        }
    }
}

/** A tool group with its plain-text caption above it. */
@Composable
private fun Section(
    title: String,
    content: @Composable RowScope.() -> Unit,
) {
    Column(verticalArrangement = Arrangement.spacedBy(CAPTION_GAP_DP.dp)) {
        BasicText(title, style = TextStyle(color = Color(CAPTION_COLOR), fontSize = CAPTION_SP.sp))
        Row(
            horizontalArrangement = Arrangement.spacedBy(GROUP_GAP_DP.dp),
            verticalAlignment = Alignment.CenterVertically,
            content = content,
        )
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
private fun RowScope.ShapeChips(state: ToolState) {
    val shapes =
        listOf(
            Triple(ShapeMode.Line, ToolIcons.Line, R.string.shape_line),
            Triple(ShapeMode.Rect, ToolIcons.Rect, R.string.shape_rect),
            Triple(ShapeMode.Ellipse, ToolIcons.Ellipse, R.string.shape_ellipse),
            Triple(ShapeMode.Arrow, ToolIcons.Arrow, R.string.shape_arrow),
        )
    shapes.forEach { (mode, icon, label) ->
        ToolButton(
            icon = icon,
            label = stringResource(label),
            selected = state.shapeMode == mode,
            onClick = { state.toggleShape(mode) },
            caption = true,
            rotation = if (mode == ShapeMode.Line) LINE_ICON_ROTATION else 0f,
        )
    }
}

@Composable
private fun RowScope.LassoChips(state: ToolState) {
    ToolButton(
        icon = ToolIcons.Lasso,
        label = stringResource(R.string.tool_lasso),
        selected = state.lassoMode,
        onClick = state::toggleLasso,
        caption = true,
    )
    if (state.selection.count > 0) {
        ToolButton(
            icon = ToolIcons.Delete,
            label = stringResource(R.string.tool_delete_selection),
            selected = false,
            onClick = state.selection::requestDelete,
            caption = true,
        )
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
