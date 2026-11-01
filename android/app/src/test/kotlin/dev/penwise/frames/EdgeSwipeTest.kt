package dev.penwise.frames

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** The edge swipe: one finger from an edge strip, inwards and mostly horizontal. */
class EdgeSwipeTest {
    // 24 dp edge, 56 dp travel and a 200 dp band at 2x density, on a 2000 x 1200 screen.
    private val swipe = EdgeSwipe(edgePx = 48f, travelPx = 112f, stripPx = 400f)

    private fun begin(
        x: Float,
        y: Float = 600f,
    ) = swipe.begin(x, y, 2000f, 1200f)

    @Test
    fun a_touch_starts_a_swipe_only_inside_an_edge_strip() {
        assertTrue(begin(0f))
        assertTrue(begin(48f))
        assertTrue(begin(1952f))
        assertTrue(begin(2000f))
        assertFalse(begin(49f))
        assertFalse(begin(1000f))
        assertFalse(swipe.active)
    }

    @Test
    fun the_strip_is_the_centred_band_the_app_excludes_from_system_gestures() {
        assertTrue(begin(10f, 400f))
        assertTrue(begin(10f, 800f))
        assertFalse(begin(10f, 399f))
        assertFalse(begin(10f, 801f))
        assertFalse(begin(1990f, 100f))
    }

    @Test
    fun from_the_left_edge_inwards_goes_to_the_previous_frame() {
        begin(10f)
        assertEquals(0, swipe.move(60f, 605f))
        assertEquals(EdgeSwipe.PREVIOUS, swipe.move(130f, 610f))
    }

    @Test
    fun from_the_right_edge_inwards_goes_to_the_next_frame() {
        begin(1990f)
        assertEquals(0, swipe.move(1900f, 600f))
        assertEquals(EdgeSwipe.NEXT, swipe.move(1870f, 580f))
    }

    @Test
    fun the_swipe_fires_once_per_touch() {
        begin(10f)
        assertEquals(EdgeSwipe.PREVIOUS, swipe.move(200f, 600f))
        assertEquals(0, swipe.move(400f, 600f))
        assertFalse(swipe.active)
    }

    @Test
    fun a_mostly_vertical_drag_from_the_edge_is_not_a_swipe() {
        begin(10f)
        assertEquals(0, swipe.move(150f, 900f))
        assertEquals(0, swipe.move(160f, 1100f))
    }

    @Test
    fun a_drag_outwards_or_too_short_is_not_a_swipe() {
        begin(30f)
        assertEquals(0, swipe.move(0f, 600f))
        assertEquals(0, swipe.move(100f, 600f))
        begin(1970f)
        assertEquals(0, swipe.move(2000f, 600f))
    }

    @Test
    fun a_touch_that_did_not_start_in_a_strip_never_fires() {
        begin(500f)
        assertEquals(0, swipe.move(1500f, 600f))
    }

    @Test
    fun resetting_ends_the_touch() {
        begin(10f)
        swipe.reset()
        assertFalse(swipe.active)
        assertEquals(0, swipe.move(300f, 600f))
    }
}
