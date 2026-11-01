package dev.penwise.frames

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** A waiting tap is never left suspended by a next touch that was not the page's; a new touch ends a fling. */
class RouterSettleTest {
    private val rig = RouterRig()
    private val host = rig.host
    private val router = rig.router

    @Test
    fun a_next_touch_near_a_waiting_tap_that_is_off_the_window_does_not_strand_the_tap() {
        rig.tap(100f, 100f)
        assertTrue(router.busy)
        host.onWindow = false
        // Close to the waiting tap, but not on a live window, and held long: not a double-tap.
        rig.down(105f, 100f)
        host.advance(DoubleTap.TAP_TIMEOUT_MS + 100)
        rig.up(105f, 100f)
        host.advance(1_000)
        assertFalse("the waiting tap was delivered or dropped, not left suspended", router.busy)
        assertEquals(listOf("DOWN 100,100", "UP 100,100"), rig.log)
    }

    @Test
    fun a_double_tap_whose_second_tap_is_off_the_window_drops_the_waiting_tap() {
        rig.tap(100f, 100f)
        host.onWindow = false
        rig.tap(105f, 100f)
        host.advance(1_000)
        assertFalse(router.busy)
        assertEquals(emptyList<String>(), rig.log)
        assertEquals(1, host.doubleTaps.size)
    }

    @Test
    fun a_touch_on_a_page_that_is_not_fitted_ends_its_fling_at_once() {
        rig.down(100f, 100f)
        assertEquals("the held touch is not sent yet, the fling stops anyway", listOf(0f to 0f), host.flings)
        assertEquals(emptyList<String>(), rig.log)
    }

    @Test
    fun a_touch_on_a_fitted_page_ends_its_fling_too() {
        host.fitted = true
        rig.down(100f, 100f)
        assertEquals(listOf(0f to 0f), host.flings)
    }
}
