package dev.penwise.ink

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** Slow drags must not flood the mirror: previews are capped to one per interval, and a new drag starts clean. */
class PreviewGateTest {
    @Test
    fun frames_inside_the_interval_are_dropped() {
        val gate = PreviewGate(40)
        assertTrue("first frame always goes", gate.allow(1_000))
        assertFalse(gate.allow(1_010))
        assertFalse(gate.allow(1_039))
        assertTrue(gate.allow(1_040))
    }

    @Test
    fun reset_lets_the_next_gesture_through_at_once() {
        val gate = PreviewGate(40)
        assertTrue(gate.allow(5_000))
        assertFalse(gate.allow(5_001))
        gate.reset()
        assertTrue(gate.allow(5_002))
    }

    @Test
    fun a_clock_reading_of_zero_still_counts_as_a_first_frame() {
        val gate = PreviewGate(40)
        assertTrue(gate.allow(0))
        assertFalse(gate.allow(10))
    }
}
