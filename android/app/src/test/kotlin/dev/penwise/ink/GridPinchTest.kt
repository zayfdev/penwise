package dev.penwise.ink

import dev.penwise.ink.GridPinch.Outcome
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** The grid pinch's dead zone: 0.82 on release, 0.70 at once, spring back otherwise, only from the fit scale. */
class GridPinchTest {
    private val pinch = GridPinch()

    private fun pinchTo(
        ratio: Float,
        steps: Int = 10,
    ): Boolean {
        val factor = Math.pow(ratio.toDouble(), 1.0 / steps).toFloat()
        var fired = false
        repeat(steps) { fired = pinch.step(factor) || fired }
        return fired
    }

    @Test
    fun a_pinch_to_0_82_opens_the_grid_on_release_not_before() {
        pinch.begin(atFitScale = true)
        assertFalse(pinchTo(0.80f))
        assertEquals(Outcome.OPEN_GRID, pinch.end())
    }

    @Test
    fun a_pinch_just_short_of_0_82_springs_back() {
        pinch.begin(atFitScale = true)
        assertFalse(pinchTo(0.84f))
        assertEquals(Outcome.SPRING_BACK, pinch.end())
    }

    @Test
    fun a_pinch_past_0_70_opens_the_grid_at_once_and_only_once() {
        pinch.begin(atFitScale = true)
        assertFalse(pinchTo(0.75f))
        assertTrue(pinch.step(0.9f))
        assertFalse("already open", pinch.step(0.9f))
        assertEquals("nothing more on release", Outcome.NONE, pinch.end())
    }

    @Test
    fun the_factor_is_accumulated_across_steps() {
        pinch.begin(atFitScale = true)
        // 0.9 * 0.9 = 0.81: no single step is under 0.82 but the gesture is.
        assertFalse(pinch.step(0.9f))
        assertFalse(pinch.step(0.9f))
        assertEquals(Outcome.OPEN_GRID, pinch.end())
    }

    @Test
    fun a_gesture_that_did_not_start_at_the_fit_scale_never_opens_the_grid() {
        pinch.begin(atFitScale = false)
        assertFalse(pinchTo(0.3f))
        assertEquals(Outcome.NONE, pinch.end())
    }

    @Test
    fun a_pinch_in_or_a_plain_pan_asks_for_nothing() {
        pinch.begin(atFitScale = true)
        pinchTo(1.5f)
        assertEquals(Outcome.NONE, pinch.end())
        pinch.begin(atFitScale = true)
        assertEquals(Outcome.NONE, pinch.end())
    }

    @Test
    fun a_gesture_starts_fresh_after_the_last_one() {
        pinch.begin(atFitScale = true)
        pinchTo(0.5f)
        pinch.end()
        pinch.begin(atFitScale = true)
        pinchTo(0.9f)
        assertEquals(Outcome.SPRING_BACK, pinch.end())
    }

    @Test
    fun the_thresholds_are_the_contracts() {
        assertEquals(0.82f, GridPinch.RELEASE_RATIO, 0f)
        assertEquals(0.70f, GridPinch.IMMEDIATE_RATIO, 0f)
    }
}
