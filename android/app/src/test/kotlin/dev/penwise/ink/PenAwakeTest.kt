package dev.penwise.ink

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** The screen stays awake for [PEN_AWAKE_MS] after the last pen input, however many samples arrive in between. */
class PenAwakeTest {
    /**
     * A virtual clock with the one-shot timer the rule expects. [advance] fires the timer at its due time; [lateBy]
     * delays that firing (a busy main thread). [input] sends a pen event stamped with the current time.
     */
    private class Harness(
        private val lateBy: Long = 0,
    ) {
        var now = 1_000L
        var awake = false
        var scheduled = 0
        var dueAt = -1L
        val rule =
            PenAwake(
                clock = { now },
                schedule = {
                    scheduled++
                    dueAt = now + it
                },
                cancel = { dueAt = -1L },
                onChange = { awake = it },
            )

        fun input() = rule.onPenInput(now)

        fun advance(ms: Long) {
            val target = now + ms
            while (dueAt >= 0 && dueAt + lateBy <= target) {
                now = dueAt + lateBy
                dueAt = -1L
                rule.check()
            }
            now = target
        }
    }

    @Test
    fun pen_input_wakes_the_screen() {
        val h = Harness()
        assertFalse(h.awake)
        h.input()
        assertTrue(h.awake)
    }

    @Test
    fun still_awake_just_before_two_minutes_and_allowed_to_sleep_at_two_minutes() {
        val h = Harness()
        h.input()
        h.advance(119_000)
        assertTrue(h.awake)
        h.advance(1_000)
        assertFalse(h.awake)
    }

    @Test
    fun new_input_at_100_seconds_extends_the_window_to_220_seconds() {
        val h = Harness()
        h.input()
        h.advance(100_000)
        h.input()
        h.advance(119_000)
        assertTrue("219 s after the first input, 119 s after the second", h.awake)
        h.advance(1_000)
        assertFalse(h.awake)
    }

    @Test
    fun input_after_sleep_is_allowed_wakes_it_again() {
        val h = Harness()
        h.input()
        h.advance(150_000)
        assertFalse(h.awake)
        h.input()
        assertTrue(h.awake)
        h.advance(120_000)
        assertFalse(h.awake)
    }

    @Test
    fun a_flood_of_samples_for_over_four_minutes_stays_awake_with_a_few_timers() {
        val h = Harness()
        repeat(25_000) {
            h.input()
            h.advance(10)
            assertTrue("awake at ${h.now}", h.awake)
        }
        assertTrue("timers armed: ${h.scheduled}", h.scheduled in 2..3)
        h.advance(119_000)
        assertTrue(h.awake)
        h.advance(1_000)
        assertFalse(h.awake)
    }

    @Test
    fun a_timer_that_fires_late_after_the_window_ran_out_sleeps_at_once() {
        val h = Harness(lateBy = 30_000)
        h.input()
        h.advance(125_000)
        assertTrue("timer not yet run: 5 s past the window, 25 s before the late firing", h.awake)
        h.advance(25_000)
        assertFalse(h.awake)
    }

    @Test
    fun input_after_stop_arms_a_new_timer() {
        val h = Harness()
        h.input()
        h.rule.stop()
        assertEquals(1, h.scheduled)
        h.advance(10_000)
        h.input()
        assertTrue(h.awake)
        assertEquals(2, h.scheduled)
        h.advance(120_000)
        assertFalse(h.awake)
    }

    @Test
    fun input_in_the_same_millisecond_the_timer_fires_keeps_the_screen_awake() {
        val h = Harness()
        h.input()
        h.now += PEN_AWAKE_MS
        h.input()
        h.rule.check()
        assertTrue(h.awake)
        h.advance(119_000)
        assertTrue(h.awake)
        h.advance(1_000)
        assertFalse(h.awake)
    }

    @Test
    fun the_timer_firing_exactly_at_the_end_of_the_window_lets_the_screen_sleep() {
        val h = Harness()
        h.input()
        h.advance(PEN_AWAKE_MS)
        assertFalse(h.awake)
    }

    @Test
    fun leaving_the_page_clears_the_wake_request_and_the_timer() {
        val h = Harness()
        h.input()
        h.rule.stop()
        assertFalse(h.awake)
        h.advance(300_000)
        assertFalse(h.awake)
    }
}
