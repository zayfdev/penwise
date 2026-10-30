package dev.penwise.frames

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/** One-finger double-tap on a frame fits it (contract §3). */
class DoubleTapTest {
    private fun tap(
        taps: DoubleTap,
        frame: String?,
        at: Long,
        x: Float = 100f,
        y: Float = 100f,
        hold: Long = 60L,
    ): String? {
        taps.down(frame, x, y, at)
        return taps.up(x, y, at + hold)
    }

    @Test
    fun two_quick_taps_on_the_same_frame_are_a_double_tap() {
        val taps = DoubleTap(slopPx = 20f)
        assertNull(tap(taps, "a", at = 0L))
        assertEquals("a", tap(taps, "a", at = 200L))
    }

    @Test
    fun taps_on_different_frames_or_far_apart_in_time_are_not() {
        val taps = DoubleTap(slopPx = 20f)
        tap(taps, "a", at = 0L)
        assertNull(tap(taps, "b", at = 200L))
        val slow = DoubleTap(slopPx = 20f)
        tap(slow, "a", at = 0L)
        assertNull(tap(slow, "a", at = 1_000L))
    }

    @Test
    fun a_drag_or_a_long_press_is_no_tap_and_off_frame_taps_count_for_nothing() {
        val taps = DoubleTap(slopPx = 20f)
        taps.down("a", 100f, 100f, 0L)
        taps.move(300f, 100f)
        assertNull(taps.up(300f, 100f, 80L))
        assertNull(tap(taps, "a", at = 200L))
        val held = DoubleTap(slopPx = 20f)
        assertNull(tap(held, "a", at = 0L, hold = 600L))
        assertNull(tap(held, "a", at = 700L))
        val off = DoubleTap(slopPx = 20f)
        assertNull(tap(off, null, at = 0L))
        assertNull(tap(off, null, at = 200L))
    }

    @Test
    fun a_second_finger_spoils_the_sequence() {
        val taps = DoubleTap(slopPx = 20f)
        tap(taps, "a", at = 0L)
        taps.down("a", 100f, 100f, 200L)
        taps.spoil()
        assertNull(taps.up(100f, 100f, 260L))
    }
}
