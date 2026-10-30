package dev.penwise.frames

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** Taps on a live page that is not fitted yet: late delivery, two different taps, double-taps, the pen. */
class TouchRouterTapTest {
    private val rig = RouterRig()
    private val host = rig.host
    private val router = rig.router
    private val log = rig.log

    private fun down(
        x: Float,
        y: Float,
    ) = rig.down(x, y)

    private fun up(
        x: Float,
        y: Float,
    ) = rig.up(x, y)

    private fun tap(
        x: Float,
        y: Float,
        holdMs: Long = 60,
    ) = rig.tap(x, y, holdMs)

    @Test
    fun a_lone_tap_reaches_the_page_after_the_double_tap_pause() {
        tap(100f, 100f)
        assertEquals(emptyList<String>(), log)
        host.advance(DoubleTap.GAP_MS + 10)
        assertEquals(listOf("DOWN 100,100", "UP 100,100"), log)
        assertTrue(host.doubleTaps.isEmpty())
    }

    @Test
    fun two_taps_on_different_buttons_are_both_delivered_in_order() {
        tap(100f, 100f)
        host.advance(100)
        // The second tap is far from the first: the first reaches the page now, the second after its own pause.
        tap(900f, 400f)
        assertEquals(listOf("DOWN 100,100", "UP 100,100"), log)
        host.advance(DoubleTap.GAP_MS + 10)
        assertEquals(listOf("DOWN 100,100", "UP 100,100", "DOWN 900,400", "UP 900,400"), log)
        assertTrue(host.doubleTaps.isEmpty())
    }

    @Test
    fun two_quick_taps_in_the_same_place_fit_the_frame_and_the_page_sees_neither() {
        tap(100f, 100f)
        host.advance(100)
        tap(105f, 102f)
        assertEquals(listOf("frame"), host.doubleTaps)
        host.advance(1_000)
        assertEquals(emptyList<String>(), log)
    }

    @Test
    fun a_second_touch_near_the_first_that_turns_out_to_be_a_press_delivers_the_first_tap_too() {
        tap(100f, 100f)
        host.advance(100)
        down(104f, 100f)
        host.advance(DoubleTap.TAP_TIMEOUT_MS + 10)
        assertEquals(listOf("DOWN 100,100", "UP 100,100", "DOWN 104,100"), log)
    }

    @Test
    fun a_fitted_page_gets_its_touches_at_once_and_owns_its_double_taps() {
        host.fitted = true
        tap(100f, 100f)
        assertEquals(listOf("DOWN 100,100", "UP 100,100"), log)
    }

    @Test
    fun the_pen_hovering_does_not_take_a_waiting_tap_away_but_the_pen_touching_ends_a_held_touch() {
        tap(100f, 100f)
        router.penDown()
        host.advance(DoubleTap.GAP_MS + 10)
        assertEquals(listOf("DOWN 100,100", "UP 100,100"), log)

        log.clear()
        down(300f, 300f)
        router.penDown()
        host.advance(1_000)
        assertEquals("the page never saw the touch the pen took over", emptyList<String>(), log)
        assertFalse(router.busy)
    }
}
