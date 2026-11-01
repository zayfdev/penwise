package dev.penwise.frames

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontFamily

/**
 * The look of the floating chrome (locked-frames contract §3.4, graphite): opaque panels with a hairline, no blur.
 * Every colour, radius and the font family live here, so a later pick of fonts or palette swaps in one place.
 */
internal object ChromeStyle {
    val panel = Color(0xFF16191F)
    val hairline = Color(0xFF2A2F38)
    val text = Color(0xFFE6E8EC)
    val textMuted = Color(0xFF9AA1AD)
    val buttonSelected = Color(0xFF2A2F38)
    val accent = Color(0xFFE6E8EC)

    const val PANEL_RADIUS_DP = 18
    const val BUTTON_RADIUS_DP = 12
    const val HAIRLINE_DP = 1

    /** The system sans until the fonts are chosen. */
    val font: FontFamily = FontFamily.SansSerif
}
