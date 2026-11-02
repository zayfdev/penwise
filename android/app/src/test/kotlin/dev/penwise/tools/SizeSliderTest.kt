package dev.penwise.tools

import org.junit.Assert.assertEquals
import org.junit.Test

class SizeSliderTest {
    private val range = 0.5f..3f

    @Test
    fun the_ends_and_the_middle_map_to_the_range() {
        assertEquals(0.5f, sliderValue(12f, 212f, 12f, range), 0.001f)
        assertEquals(3f, sliderValue(200f, 212f, 12f, range), 0.001f)
        assertEquals(1.75f, sliderValue(106f, 212f, 12f, range), 0.001f)
    }

    @Test
    fun a_drag_off_either_end_is_held_in_range() {
        assertEquals(0.5f, sliderValue(-40f, 212f, 12f, range), 0.001f)
        assertEquals(3f, sliderValue(900f, 212f, 12f, range), 0.001f)
    }
}
