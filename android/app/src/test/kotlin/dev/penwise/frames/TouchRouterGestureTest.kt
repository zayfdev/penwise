package dev.penwise.frames

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** Presses, drags, the canvas taking a touch, and what is left for the pen to cancel. */
class TouchRouterGestureTest {
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
    fun a_finger_held_still_past_the_tap_time_is_handed_to_the_page_as_a_press() {
        down(100f, 100f)
        host.advance(100)
        assertEquals(emptyList<String>(), log)
        host.advance(DoubleTap.TAP_TIMEOUT_MS)
        // The page owns the gesture from its first point: long-press, selection and :active work.
        assertEquals(listOf("DOWN 100,100"), log)
        assertTrue(router.touching)
        assertEquals(1, host.fingerDowns)
        up(100f, 100f)
        assertEquals(listOf("DOWN 100,100", "UP 100,100"), log)
        host.advance(1_000)
        assertEquals("no replayed tap on top of it", 2, log.size)
    }

    @Test
    fun a_drag_reaches_the_page_from_where_the_finger_went_down() {
        down(100f, 100f)
        host.advance(30)
        assertTrue(router.onMove(104f, 100f, rig.clock))
        assertEquals(emptyList<String>(), log)
        assertTrue(router.onMove(160f, 100f, rig.clock))
        assertEquals(listOf("DOWN 100,100", "MOVE 160,100"), log)
        up(160f, 100f)
        assertEquals("UP 160,100", log.last())
        assertEquals(1, host.fingerUps)
    }

    @Test
    fun a_second_finger_takes_a_held_touch_for_the_canvas_without_the_page_seeing_it() {
        down(100f, 100f)
        assertTrue(router.takeForCanvas())
        host.advance(1_000)
        assertEquals(emptyList<String>(), log)
        assertFalse(up(100f, 100f))
    }

    @Test
    fun a_forwarded_touch_is_cancelled_when_the_canvas_takes_it() {
        host.fitted = true
        down(100f, 100f)
        assertTrue(router.takeForCanvas())
        assertEquals("CANCEL 100,100", log.last())
        assertEquals(1, host.fingerUps)
    }

    @Test
    fun busy_says_whether_there_is_anything_for_the_pen_to_cancel() {
        assertFalse(router.busy)
        down(100f, 100f)
        assertTrue(router.busy)
        up(100f, 100f)
        assertTrue("a tap is waiting to be replayed", router.busy)
        host.advance(DoubleTap.GAP_MS + 10)
        assertFalse(router.busy)
    }
}
