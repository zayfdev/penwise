package dev.penwise.ink

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** The live preview keeps a point every few pixels, no matter how many events the pen delivers. */
class PreviewThinnerTest {
    @Test
    fun the_first_point_is_always_kept() {
        val thinner = PreviewThinner(4f)
        assertTrue(thinner.keep(100f, 100f))
    }

    @Test
    fun points_closer_than_the_spacing_are_dropped() {
        val thinner = PreviewThinner(4f)
        assertTrue(thinner.keep(100f, 100f))
        assertFalse("1 px away", thinner.keep(101f, 100f))
        assertFalse("3 px away", thinner.keep(103f, 100f))
        assertTrue("4 px away", thinner.keep(104f, 100f))
    }

    @Test
    fun movement_on_either_axis_counts() {
        val thinner = PreviewThinner(4f)
        assertTrue(thinner.keep(0f, 0f))
        assertTrue(thinner.keep(0f, 5f))
        assertFalse(thinner.keep(2f, 6f))
    }

    @Test
    fun a_hundred_events_in_one_spot_collapse_to_one_point() {
        val thinner = PreviewThinner(4f)
        var kept = 0
        repeat(100) { i -> if (thinner.keep(50f + (i % 3), 50f)) kept++ }
        assertTrue("kept $kept of 100", kept <= 2)
    }

    @Test
    fun reset_starts_the_next_stroke_fresh() {
        val thinner = PreviewThinner(4f)
        assertTrue(thinner.keep(0f, 0f))
        assertFalse(thinner.keep(1f, 1f))
        thinner.reset()
        assertTrue("a new stroke's first point always shows", thinner.keep(1f, 1f))
    }
}
