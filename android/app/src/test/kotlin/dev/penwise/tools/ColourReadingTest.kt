package dev.penwise.tools

import org.junit.Assert.assertEquals
import org.junit.Test

class ColourReadingTest {
    @Test
    fun the_core_names_map_to_the_flyout_wording() {
        assertEquals(ColourReading.PROBLEM, ColourReading.of("problem"))
        assertEquals(ColourReading.IDEA, ColourReading.of("idea"))
        assertEquals(ColourReading.KEEP, ColourReading.of("good"))
        assertEquals(ColourReading.NOTE, ColourReading.of("none"))
    }

    @Test
    fun an_unknown_name_is_a_plain_note() {
        assertEquals(ColourReading.NOTE, ColourReading.of("whatever"))
    }
}
