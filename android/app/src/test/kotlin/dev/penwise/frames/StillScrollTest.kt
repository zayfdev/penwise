package dev.penwise.frames

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** One finger anywhere on a locked frame's box scrolls the live page, like a browser; taps stay taps. */
class StillScrollTest {
    private val rig = StillRig()
    private val host = rig.host
    private val router = rig.router

    private fun drag(
        steps: Int,
        dyPerStep: Float,
        stepMs: Long = 10,
    ) = rig.drag(steps, dyPerStep, stepMs)

    @Test
    fun a_drag_on_the_still_part_scrolls_the_page_by_the_finger() {
        drag(steps = 6, dyPerStep = -30f)
        // The first report covers the travel from the down point, the rest follow the finger one to one.
        assertEquals(-180f, host.scrolls.sumOf { it.second.toDouble() }.toFloat(), 0.01f)
        assertEquals(1, host.fingerDowns)
        assertTrue(router.busy)
        assertTrue(router.onUp(500f, 1_320f, host.clock))
        assertEquals(1, host.fingerUps)
    }

    @Test
    fun a_quick_flick_flings_the_page_on_with_the_finger_velocity() {
        drag(steps = 6, dyPerStep = -30f, stepMs = 10)
        router.onUp(500f, 1_320f, host.clock + 5)
        val (vx, vy) = host.flings.last()
        assertEquals(0f, vx, 0.01f)
        assertEquals(-3_000f, vy, 150f)
    }

    @Test
    fun a_finger_that_rests_before_lifting_does_not_fling() {
        drag(steps = 6, dyPerStep = -30f)
        host.clock += 300
        router.onUp(500f, 1_320f, host.clock)
        assertTrue(DragVelocity(host.flings.last().first, host.flings.last().second).isRest)
    }

    @Test
    fun a_tap_on_the_still_part_stays_a_tap() {
        assertFalse(router.onDown(500f, 1_500f, 1_000, 1_000))
        assertFalse(router.onMove(505f, 1_503f, 1_020))
        assertFalse("the canvas gets the lift too", router.onUp(505f, 1_503f, 1_060))
        assertTrue(host.scrolls.isEmpty())
        assertEquals(0, host.fingerDowns)
        assertEquals(0, host.fingerUps)
    }

    @Test
    fun a_touch_on_the_page_ends_the_fling_it_was_in() {
        router.onDown(500f, 1_500f, 1_000, 1_000)
        assertEquals(listOf(0f to 0f), host.flings)
    }

    @Test
    fun a_second_finger_hands_the_scroll_to_the_canvas_for_zoom() {
        drag(steps = 4, dyPerStep = -30f)
        val taken = router.takeForCanvas()
        assertTrue(taken)
        assertEquals(1, host.fingerUps)
        val before = host.scrolls.size
        assertFalse(router.onMove(500f, 1_300f, host.clock + 10))
        assertEquals(before, host.scrolls.size)
    }
}
