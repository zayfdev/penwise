package dev.penwise.frames

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.ReadOnlyComposable
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontFamily

/**
 * One set of chrome colours (locked-frames contract §3.4, graphite): opaque panels with a hairline, no blur. The dark
 * and the light set carry the same roles, so a control reads its role and never a literal colour.
 */
@Immutable
internal class ChromePalette(
    /** Dock, flyout and chip surface. */
    val panel: Color,
    val hairline: Color,
    val text: Color,
    val textMuted: Color,
    /** A selected dock button's fill. */
    val buttonSelected: Color,
    val accent: Color,
    /** An idle icon, and the icon of a selected control. */
    val icon: Color,
    val iconOn: Color,
    /** A tile or toggle inside a flyout, idle and selected, each with its outline. */
    val tileFill: Color,
    val tileBorder: Color,
    val tileSelectedFill: Color,
    val tileSelectedBorder: Color,
    /** The thin rule between dock groups. */
    val divider: Color,
    /** The empty part of a slider track. */
    val track: Color,
    /** The primary pill (Send): filled with [accent], its label in [onAccent]. */
    val onAccent: Color,
    /** The well behind the colour wheel's square. */
    val well: Color,
    /** The sync dot: linked, connecting, offline. Each reads at 3:1 or better on [panel]. */
    val online: Color,
    val connecting: Color,
    val offline: Color,
)

/**
 * The look of the floating chrome. Every colour, radius and the font family live here, so a later pick of fonts or
 * palette swaps in one place. The colours follow the system light/dark setting.
 */
internal object ChromeStyle {
    val dark =
        ChromePalette(
            panel = Color(0xFF16191F),
            hairline = Color(0xFF2A2F38),
            text = Color(0xFFE6E8EC),
            textMuted = Color(0xFF9AA1AD),
            buttonSelected = Color(0xFF2A2F38),
            accent = Color(0xFFE6E8EC),
            icon = Color(0xFFC9CFDB),
            iconOn = Color(0xFFFFFFFF),
            tileFill = Color(0x0AFFFFFF),
            tileBorder = Color(0x1AFFFFFF),
            tileSelectedFill = Color(0x1FFFFFFF),
            tileSelectedBorder = Color(0x47FFFFFF),
            divider = Color(0x1FFFFFFF),
            track = Color(0x24FFFFFF),
            onAccent = Color(0xFF0C0F14),
            well = Color(0xFF1B1E26),
            online = Color(0xFF4ADE80),
            connecting = Color(0xFFE0A100),
            offline = Color(0xFFE5534B),
        )

    /** Graphite on light: the same roles with the ink and the paper swapped. */
    val light =
        ChromePalette(
            panel = Color(0xFFF4F5F7),
            hairline = Color(0xFFD3D7DE),
            text = Color(0xFF16191F),
            textMuted = Color(0xFF5A6170),
            buttonSelected = Color(0xFFDFE2E8),
            accent = Color(0xFF16191F),
            icon = Color(0xFF3A404B),
            iconOn = Color(0xFF0C0F14),
            tileFill = Color(0x0A000000),
            tileBorder = Color(0x1A000000),
            tileSelectedFill = Color(0x14000000),
            tileSelectedBorder = Color(0x4D000000),
            divider = Color(0x1F000000),
            track = Color(0x24000000),
            onAccent = Color(0xFFF4F5F7),
            well = Color(0xFFE8EAEE),
            online = Color(0xFF15803D),
            connecting = Color(0xFF9A6700),
            offline = Color(0xFFC62828),
        )

    /** The palette for the system setting. */
    fun palette(darkTheme: Boolean): ChromePalette = if (darkTheme) dark else light

    /** The colours to draw with now: they follow the system light/dark setting. */
    val colors: ChromePalette
        @Composable
        @ReadOnlyComposable
        get() = palette(isSystemInDarkTheme())

    const val PANEL_RADIUS_DP = 18
    const val BUTTON_RADIUS_DP = 12
    const val HAIRLINE_DP = 1

    /** The system sans until the fonts are chosen. */
    val font: FontFamily = FontFamily.SansSerif

    /** The system monospace for readouts (a size, a hex code) until the fonts are chosen. */
    val mono: FontFamily = FontFamily.Monospace
}
