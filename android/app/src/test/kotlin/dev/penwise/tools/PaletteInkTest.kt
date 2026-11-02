package dev.penwise.tools

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class PaletteInkTest {
    @Test
    fun ink_black_needs_an_outline_but_meaning_colours_and_graphite_do_not() {
        assertTrue(Palette.isDarkInk(Palette.INK_BLACK))
        assertFalse(Palette.isDarkInk(Palette.GRAPHITE))
        Palette.pinned.forEach { assertFalse(Palette.isDarkInk(it)) }
    }
}
