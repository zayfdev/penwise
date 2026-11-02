package dev.penwise.tools

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** The wheel's HSV maths against the RGB the pen writes with. */
class ColourMathTest {
    @Test
    fun the_primary_hues_are_the_primary_colours() {
        assertEquals(0xFF0000, ColourMath.toRgb(Hsv(0f, 1f, 1f)))
        assertEquals(0x00FF00, ColourMath.toRgb(Hsv(120f, 1f, 1f)))
        assertEquals(0x0000FF, ColourMath.toRgb(Hsv(240f, 1f, 1f)))
        assertEquals(0xFFFF00, ColourMath.toRgb(Hsv(60f, 1f, 1f)))
    }

    @Test
    fun white_and_black_sit_at_the_square_corners() {
        assertEquals(0xFFFFFF, ColourMath.toRgb(Hsv(200f, 0f, 1f)))
        assertEquals(0x000000, ColourMath.toRgb(Hsv(200f, 1f, 0f)))
    }

    @Test
    fun a_hue_past_a_full_turn_wraps() {
        assertEquals(ColourMath.toRgb(Hsv(30f, 1f, 1f)), ColourMath.toRgb(Hsv(390f, 1f, 1f)))
        assertEquals(ColourMath.toRgb(Hsv(350f, 1f, 1f)), ColourMath.toRgb(Hsv(-10f, 1f, 1f)))
    }

    @Test
    fun every_colour_survives_a_trip_through_the_wheel() {
        for (rgb in listOf(
            Palette.RED,
            Palette.BLUE,
            Palette.GREEN,
            Palette.ORANGE,
            Palette.PURPLE,
            0x123456,
            0xFEDCBA,
        )) {
            val plain = rgb and 0xFFFFFF
            assertEquals(plain, ColourMath.toRgb(ColourMath.fromRgb(plain)))
        }
    }

    @Test
    fun a_grey_has_no_saturation() {
        val grey = ColourMath.fromRgb(Palette.GRAPHITE)
        assertEquals(0f, grey.saturation, 0f)
    }

    @Test
    fun the_pinned_colours_sit_in_the_hue_bands_the_server_reads() {
        // penwise-core marks::colour: red near 0, green near 130, blue near 220 degrees.
        val (red, blue, green) = Palette.pinned.map { ColourMath.fromRgb(it) }
        assertTrue(red.hue < 20f || red.hue > 340f)
        assertTrue(green.hue in 110f..150f)
        assertTrue(blue.hue in 200f..240f)
    }

    @Test
    fun hex_is_six_capital_digits() {
        assertEquals("#D62F2F", ColourMath.hex(Palette.RED))
        assertEquals("#000001", ColourMath.hex(1))
    }

    @Test
    fun a_ring_drag_on_near_black_ink_lifts_it_to_a_visible_colour() {
        val ink = ColourMath.fromRgb(0x14141E)
        val dragged = ColourMath.withHue(ink, 120f)
        assertEquals(120f, dragged.hue, 0f)
        assertEquals(1f, dragged.saturation, 0f)
        assertEquals(1f, dragged.value, 0f)
        assertEquals(0x00FF00, ColourMath.toRgb(dragged))
    }

    @Test
    fun a_ring_drag_keeps_the_saturation_and_value_of_a_real_colour() {
        val colour = Hsv(10f, 0.6f, 0.8f)
        val dragged = ColourMath.withHue(colour, 200f)
        assertEquals(0.6f, dragged.saturation, 0f)
        assertEquals(0.8f, dragged.value, 0f)
    }

    @Test
    fun black_greys_and_near_black_ink_have_no_hue_to_mark_on_the_ring() {
        assertFalse(ColourMath.hasHue(ColourMath.fromRgb(Palette.INK_BLACK)))
        assertFalse(ColourMath.hasHue(ColourMath.fromRgb(Palette.GRAPHITE)))
        assertFalse(ColourMath.hasHue(ColourMath.fromRgb(0x14141E)))
        assertFalse(ColourMath.hasHue(ColourMath.fromRgb(0xFFFFFF)))
    }

    @Test
    fun real_colours_have_a_hue_to_mark_on_the_ring() {
        assertTrue(ColourMath.hasHue(ColourMath.fromRgb(Palette.RED)))
        assertTrue(ColourMath.hasHue(ColourMath.fromRgb(Palette.BLUE)))
        assertTrue(ColourMath.hasHue(ColourMath.fromRgb(Palette.GREEN)))
        assertTrue(ColourMath.hasHue(ColourMath.fromRgb(Palette.ORANGE)))
    }
}
