package dev.penwise.frames

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** When the still part must not scroll the page: a palm, an edge swipe, a second finger, the pen, a cancel. */
class StillGuardTest {
    private val rig = StillRig()
    private val host = rig.host
    private val router = rig.router

    private fun drag(
        steps: Int,
        dyPerStep: Float,
        stepMs: Long = 10,
    ) = rig.drag(steps, dyPerStep, stepMs)

    @Test
    fun the_pen_ends_the_scroll_without_a_fling() {
        drag(steps = 4, dyPerStep = -30f)
        router.penDown()
        assertEquals(1, host.fingerUps)
        assertFalse(router.busy)
        assertTrue("the pen starts no fling", host.flings.none { !DragVelocity(it.first, it.second).isRest })
    }

    @Test
    fun off_the_locked_frame_the_finger_is_the_canvas_s() {
        host.stillPage = null
        assertFalse(router.onDown(500f, 1_500f, 1_000, 1_000))
        assertFalse(router.onMove(500f, 1_300f, 1_020))
        assertTrue(host.scrolls.isEmpty())
    }

    @Test
    fun a_cancelled_scroll_lets_go_of_the_page() {
        drag(steps = 3, dyPerStep = -30f)
        assertTrue(router.onCancel())
        assertEquals(1, host.fingerUps)
        assertFalse(router.busy)
    }

    @Test
    fun a_finger_right_after_the_pen_is_a_palm_and_scrolls_nothing() {
        host.penRecent = true
        assertFalse(router.onDown(500f, 1_500f, 1_000, 1_000))
        assertFalse(router.onMove(500f, 1_300f, 1_020))
        assertTrue(host.scrolls.isEmpty())
        assertFalse(router.busy)
    }

    @Test
    fun a_sideways_touch_from_an_edge_strip_scrolls_nothing_until_it_leans_up_or_down() {
        host.hold = true
        router.onDown(5f, 1_500f, 1_000, 1_000)
        assertFalse(router.onMove(5f, 1_450f, 1_020))
        assertTrue(host.scrolls.isEmpty())
        host.hold = false
        assertTrue(router.onMove(5f, 1_400f, 1_040))
        assertEquals(-100f, host.scrolls.sumOf { it.second.toDouble() }.toFloat(), 0.01f)
    }

    @Test
    fun a_second_finger_before_the_slop_scrolls_nothing_and_the_canvas_takes_both() {
        router.onDown(500f, 1_500f, 1_000, 1_000)
        assertTrue("the canvas is handed the gesture, so a two-finger tap still undoes", router.takeForCanvas())
        assertFalse(router.onMove(500f, 1_400f, 1_020))
        assertTrue(host.scrolls.isEmpty())
        assertEquals(0, host.fingerDowns)
    }

    @Test
    fun a_double_tap_on_the_still_part_fits_the_frame() {
        router.onDown(500f, 1_500f, 1_000, 1_000)
        router.onUp(500f, 1_500f, 1_050)
        router.onDown(502f, 1_500f, 1_200, 1_200)
        router.onUp(502f, 1_500f, 1_250)
        assertEquals(1, host.doubleTaps)
        assertTrue(host.scrolls.isEmpty())
    }

    @Test
    fun a_reset_in_the_middle_of_a_scroll_ends_the_owner_scroll() {
        drag(steps = 3, dyPerStep = -30f)
        router.reset()
        assertEquals(host.fingerDowns, host.fingerUps)
    }
}
