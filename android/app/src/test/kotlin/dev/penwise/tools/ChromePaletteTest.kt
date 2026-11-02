package dev.penwise.tools

import androidx.compose.ui.graphics.Color
import dev.penwise.frames.ChromeStyle
import org.junit.Assert.assertEquals
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test
import kotlin.math.pow

/** The chrome tokens: the system setting picks the set, and each set keeps its text readable on its panel. */
class ChromePaletteTest {
    @Test
    fun the_system_setting_picks_the_set() {
        assertSame(ChromeStyle.dark, ChromeStyle.palette(darkTheme = true))
        assertSame(ChromeStyle.light, ChromeStyle.palette(darkTheme = false))
    }

    @Test
    fun the_dark_set_is_the_approved_graphite() {
        assertEquals(Color(0xFF16191F), ChromeStyle.dark.panel)
        assertEquals(Color(0xFF2A2F38), ChromeStyle.dark.hairline)
    }

    @Test
    fun text_and_icons_read_on_the_panel_in_both_sets() {
        for (palette in listOf(ChromeStyle.dark, ChromeStyle.light)) {
            assertTrue(contrast(palette.text, palette.panel) >= 7.0)
            assertTrue(contrast(palette.icon, palette.panel) >= 4.5)
            assertTrue(contrast(palette.textMuted, palette.panel) >= 4.5)
            assertTrue(contrast(palette.onAccent, palette.accent) >= 7.0)
        }
    }

    @Test
    fun the_sync_dot_reads_on_the_panel_in_both_sets() {
        for (palette in listOf(ChromeStyle.dark, ChromeStyle.light)) {
            for (dot in listOf(palette.online, palette.connecting, palette.offline)) {
                assertTrue(contrast(dot, palette.panel) >= 3.0)
            }
        }
    }

    private fun contrast(
        a: Color,
        b: Color,
    ): Double {
        val (high, low) = listOf(luminance(a), luminance(b)).sortedDescending()
        return (high + 0.05) / (low + 0.05)
    }

    private fun luminance(c: Color): Double {
        fun lin(v: Float): Double = if (v <= 0.03928f) v / 12.92 else ((v + 0.055) / 1.055).pow(2.4)
        return 0.2126 * lin(c.red) + 0.7152 * lin(c.green) + 0.0722 * lin(c.blue)
    }
}
