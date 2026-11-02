package dev.penwise.tools

import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import dev.penwise.frames.ChromeStyle

private const val TILE_RADIUS_DP = 14
private const val TOGGLE_RADIUS_DP = 10
private const val ACCENT_FILL = 0.16f
private const val ACCENT_LINE = 0.6f
private const val TITLE_SP = 13
private const val READOUT_SP = 11
private const val TOGGLE_SP = 11

/** A flyout's title row: the [title] on the left and a small monospace [readout] on the right. */
@Composable
internal fun FlyoutHeader(
    title: String,
    readout: String,
) {
    val colors = ChromeStyle.colors
    Row(
        Modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.SpaceBetween,
        verticalAlignment = Alignment.Bottom,
    ) {
        DockText(title, colors.text, TITLE_SP, weight = FontWeight.SemiBold)
        DockText(readout, colors.textMuted, READOUT_SP, family = ChromeStyle.mono)
    }
}

/** What a [FlyoutTile] shows: [label] (also the screen-reader name), [selected], [height] and an optional [accent]. */
internal class TileSpec(
    val label: String,
    val selected: Boolean,
    val height: Dp,
    val accent: Color? = null,
)

/**
 * A selectable tile (a pen, a shape): fills and outlines when selected (in the spec's accent when given), shrinks
 * while pressed.
 */
@Composable
internal fun FlyoutTile(
    spec: TileSpec,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    content: @Composable ColumnScope.() -> Unit,
) {
    val colors = ChromeStyle.colors
    val source = remember { MutableInteractionSource() }
    val animation = tween<Color>(DockMotion.SELECT_MS, easing = DockMotion.easeOut)
    val selectedFill = spec.accent?.copy(alpha = ACCENT_FILL) ?: colors.tileSelectedFill
    val selectedLine = spec.accent?.copy(alpha = ACCENT_LINE) ?: colors.tileSelectedBorder
    val fill by animateColorAsState(if (spec.selected) selectedFill else colors.tileFill, animation, label = "tileFill")
    val line by animateColorAsState(
        if (spec.selected) selectedLine else colors.tileBorder,
        animation,
        label = "tileLine",
    )
    val shape = RoundedCornerShape(TILE_RADIUS_DP.dp)
    Column(
        modifier
            .height(spec.height)
            .pressScale(source)
            .clip(shape)
            .background(fill)
            .border(ChromeStyle.HAIRLINE_DP.dp, line, shape)
            .semantics {
                contentDescription = spec.label
                this.selected = spec.selected
            }.clickable(interactionSource = source, indication = null, role = Role.Button, onClick = onClick),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.Center,
        content = content,
    )
}

/** A small on/off pill under a flyout's controls; a screen reader announces it as a switch with its state. */
@Composable
internal fun FlyoutToggle(
    label: String,
    on: Boolean,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val colors = ChromeStyle.colors
    val source = remember { MutableInteractionSource() }
    val fill by animateColorAsState(
        if (on) colors.tileSelectedFill else colors.tileFill,
        tween(DockMotion.SELECT_MS, easing = DockMotion.easeOut),
        label = "toggleFill",
    )
    Row(
        modifier
            .pressScale(source)
            .clip(RoundedCornerShape(TOGGLE_RADIUS_DP.dp))
            .background(fill)
            .toggleable(
                value = on,
                interactionSource = source,
                indication = null,
                role = Role.Switch,
                onValueChange = { onClick() },
            ).padding(horizontal = 10.dp, vertical = 6.dp),
    ) {
        DockText(label, if (on) colors.text else colors.textMuted, TOGGLE_SP)
    }
}

private const val GRID_GAP_DP = 8

/** [items] laid out [columns] to a row with equal-width tiles; a short last row keeps the tile width. */
@Composable
internal fun <T> TileGrid(
    items: List<T>,
    columns: Int,
    tile: @Composable RowScope.(T) -> Unit,
) {
    Column(verticalArrangement = Arrangement.spacedBy(GRID_GAP_DP.dp)) {
        for (row in items.chunked(columns)) {
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(GRID_GAP_DP.dp)) {
                for (item in row) tile(item)
                repeat(columns - row.size) { Spacer(Modifier.weight(1f)) }
            }
        }
    }
}
