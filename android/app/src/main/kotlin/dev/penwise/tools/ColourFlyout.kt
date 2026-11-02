package dev.penwise.tools

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import dev.penwise.R
import dev.penwise.core.Native
import dev.penwise.frames.ChromeStyle

private const val PANEL_WIDTH_DP = 400
private const val WHEEL_DP = 200
private const val MEANING_HEIGHT_DP = 36
private const val DOT_DP = 12
private const val RECENT_DP = 28
private const val LABEL_SP = 12
private const val CAPTION_SP = 11
private const val OPAQUE = -0x1000000
private const val RGB_MASK = 0xFFFFFF
private const val RING_LINE_DP = 2

/**
 * The colour flyout: the hue wheel for any colour, the three pinned colours with their meanings (red = problem,
 * blue = idea, green = keep: the only ones the agent reads), ink black and graphite, and the recent colours. Any other
 * colour is a plain note. The header says what the current colour reads as, asked of the core, never worked out here.
 */
@Composable
internal fun ColourFlyout(state: ToolState) {
    val current = state.argb() and RGB_MASK
    FlyoutPanel(PANEL_WIDTH_DP.dp, gap = 14.dp) {
        FlyoutHeader(
            stringResource(R.string.colour_title),
            "${stringResource(
                ColourReading.of(Native.colourMeaning(state.argb())).label,
            )} · ${ColourMath.hex(current)}",
        )
        Row(horizontalArrangement = Arrangement.spacedBy(16.dp), verticalAlignment = Alignment.CenterVertically) {
            ColourWheel(
                rgb = current,
                onPick = state::selectSwatch,
                onEnd = state::commitColour,
                label = stringResource(R.string.colour_wheel),
                side = WHEEL_DP.dp,
            )
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                Caption(stringResource(R.string.colour_meaning))
                val names = listOf(R.string.colour_problem, R.string.colour_idea, R.string.colour_keep)
                Palette.pinned.zip(names).forEach { (rgb, name) ->
                    MeaningButton(rgb, stringResource(name), state.swatchRgb == (rgb and RGB_MASK)) {
                        state.selectSwatch(rgb)
                    }
                }
                Caption(stringResource(R.string.colour_ink))
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    Palette.inkPins.forEach { rgb ->
                        RecentDot(rgb and RGB_MASK, state.swatchRgb == (rgb and RGB_MASK)) { state.selectSwatch(rgb) }
                    }
                }
            }
        }
        Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Caption(stringResource(R.string.colour_recent))
            Row(
                Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                AutoChip(state.swatchRgb == null) { state.selectSwatch(null) }
                state.recents.items.forEach { rgb ->
                    RecentDot(rgb, state.swatchRgb == rgb) { state.selectSwatch(rgb) }
                }
            }
        }
    }
}

@Composable
private fun Caption(text: String) {
    DockText(text.uppercase(), ChromeStyle.colors.textMuted, CAPTION_SP)
}

/** One pinned colour: its dot and its meaning, lit in its own colour while it is the pen's colour. */
@Composable
private fun MeaningButton(
    rgb: Int,
    label: String,
    selected: Boolean,
    onClick: () -> Unit,
) {
    val colors = ChromeStyle.colors
    val dot = Color(rgb or OPAQUE)
    FlyoutTile(TileSpec(label, selected, MEANING_HEIGHT_DP.dp, dot), onClick, Modifier.fillMaxWidth()) {
        Row(
            Modifier.fillMaxWidth().padding(horizontal = 10.dp),
            horizontalArrangement = Arrangement.spacedBy(8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Box(Modifier.size(DOT_DP.dp).background(dot, CircleShape))
            DockText(label, if (selected) colors.text else colors.icon, LABEL_SP)
        }
    }
}

/** The pen's own colour for the chosen brush (no override). */
@Composable
private fun AutoChip(
    selected: Boolean,
    onClick: () -> Unit,
) {
    FlyoutToggle(stringResource(R.string.tool_preset_color), selected, onClick)
}

@Composable
private fun RecentDot(
    rgb: Int,
    selected: Boolean,
    onClick: () -> Unit,
) {
    val colors = ChromeStyle.colors
    Box(
        Modifier
            .size(RECENT_DP.dp)
            .semantics { contentDescription = ColourMath.hex(rgb) }
            .clip(CircleShape)
            .background(Color(rgb or OPAQUE))
            .border(RING_LINE_DP.dp, if (selected) colors.accent else if (Palette.isDarkInk(rgb)) colors.textMuted else colors.tileBorder, CircleShape)
            .clickable(role = Role.RadioButton, onClick = onClick),
    )
}
