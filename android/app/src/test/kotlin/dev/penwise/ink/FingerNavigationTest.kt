package dev.penwise.ink

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** Two-finger tap (undo) versus two-finger drag/pinch (navigate): never both, one finger never anything. */
class FingerNavigationTest {
    private var taps = 0
    private val steps = ArrayList<NavStep>()
    private val active = ArrayList<Boolean>()
    private val nav = FingerNavigation(SLOP, { taps++ }, { steps += it }, { active += it })

    private fun twoDown() {
        nav.down(0, 400f, 500f, 0)
        nav.down(1, 600f, 500f, 20)
    }

    private fun moveTo(
        x0: Float,
        y0: Float,
        x1: Float,
        y1: Float,
    ) {
        nav.move(0, x0, y0)
        nav.move(1, x1, y1)
        nav.update()
    }

    @Test
    fun a_quick_two_finger_tap_fires_once() {
        twoDown()
        nav.up(0, 90)
        nav.up(1, 100)
        assertEquals(1, taps)
        assertTrue(steps.isEmpty())
    }

    @Test
    fun jitter_inside_the_slop_is_still_a_tap() {
        twoDown()
        moveTo(405f, 503f, 596f, 498f)
        nav.up(0, 90)
        nav.up(1, 100)
        assertEquals(1, taps)
        assertTrue(steps.isEmpty())
    }

    @Test
    fun a_slow_two_finger_touch_is_not_a_tap() {
        twoDown()
        nav.up(0, 500)
        nav.up(1, 510)
        assertEquals(0, taps)
    }

    @Test
    fun one_finger_never_does_anything() {
        nav.down(0, 400f, 500f, 0)
        nav.move(0, 900f, 900f)
        nav.update()
        nav.up(0, 50)
        assertEquals(0, taps)
        assertTrue(steps.isEmpty())
        assertTrue(active.isEmpty())
    }

    @Test
    fun a_third_finger_spoils_the_tap_and_the_navigation() {
        twoDown()
        nav.down(2, 800f, 500f, 30)
        moveTo(500f, 520f, 700f, 520f)
        nav.up(2, 60)
        nav.up(0, 70)
        nav.up(1, 80)
        assertEquals(0, taps)
        assertTrue(steps.isEmpty())
    }

    @Test
    fun cancelling_drops_the_session_without_a_tap() {
        twoDown()
        nav.cancel()
        nav.up(0, 50)
        nav.up(1, 60)
        assertEquals(0, taps)
    }

    private companion object {
        const val SLOP = 24f
    }
}
