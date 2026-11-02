package dev.penwise.tools

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** The colour square for ink with and without a hue. */
class ColourSquareTest {
    @Test
    fun the_square_is_a_pure_grey_ramp_for_ink_without_a_hue() {
        val nearBlack = ColourMath.fromRgb(0x1A1A1A)
        assertFalse(ColourMath.hasHue(nearBlack))
        assertEquals(0xFFFFFF, ColourMath.squareEdge(nearBlack))
        assertEquals(0xFFFFFF, ColourMath.squareEdge(ColourMath.fromRgb(0x808080)))
    }

    @Test
    fun the_square_edge_is_the_pure_hue_for_ink_with_one() {
        val red = ColourMath.fromRgb(0xD32F2F)
        assertTrue(ColourMath.hasHue(red))
        assertEquals(ColourMath.toRgb(Hsv(red.hue, 1f, 1f)), ColourMath.squareEdge(red))
    }

    @Test
    fun a_square_drag_keeps_saturation_at_zero_while_the_ink_has_no_hue() {
        assertEquals(0f, ColourMath.squareSaturation(0.8f, withHue = false), 0f)
        assertEquals(0.8f, ColourMath.squareSaturation(0.8f, withHue = true), 0f)
    }
}
