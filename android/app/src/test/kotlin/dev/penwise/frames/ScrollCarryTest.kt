package dev.penwise.frames

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** The pure parts of the still scroll: drag to view px, fling velocity, the zone, the palm window. */
class ScrollCarryTest {
    @Test
    fun the_zone_is_the_locked_live_frame_s_box_outside_its_window() {
        assertTrue(StillZone.scrolls(true, false, "f", "f", "f"))
        assertFalse("on the window", StillZone.scrolls(true, true, "f", "f", "f"))
        assertFalse("not locked", StillZone.scrolls(true, false, "f", "f", null))
        assertFalse("another frame is locked", StillZone.scrolls(true, false, "f", "f", "g"))
        assertFalse("another frame's box", StillZone.scrolls(true, false, "g", "f", "f"))
        assertFalse("hidden", StillZone.scrolls(false, false, "f", "f", "f"))
    }

    @Test
    fun a_drag_converts_to_whole_view_px_without_losing_the_rest() {
        val carry = ScrollCarry()
        // Camera scale 0.5 screen px per view px: 3 screen px = 6 view px up; 1 px at a time carries the halves.
        assertEquals(0 to -6, carry.viewPx(0f, 3f, 0.5f))
        var total = 0
        repeat(10) { total += carry.viewPx(0f, -1f, 3f).second }
        assertEquals("10 px at 3 px per view px", 3, total)
    }

    @Test
    fun a_drag_never_takes_the_page_past_its_scroll_range() {
        val carry = ScrollCarry()
        val top = ScrollBounds(0, 20, 0, 1_000)
        assertEquals("up to 0, no overshoot", 0 to -20, carry.viewPx(0f, 100f, 1f, top))
        val end = ScrollBounds(0, 990, 0, 1_000)
        assertEquals(0 to 10, carry.viewPx(0f, -100f, 1f, end))
        // The lost rest is dropped: coming back is not delayed.
        assertEquals(0 to 5, carry.viewPx(0f, -5f, 1f, ScrollBounds(0, 0, 0, 1_000)))
    }

    @Test
    fun a_fling_is_the_finger_velocity_in_view_px_against_the_content() {
        val carry = ScrollCarry()
        assertEquals(0 to 6_000, carry.flingView(0f, -3_000f, 0.5f))
        assertEquals(-1_000 to 0, carry.flingView(2_000f, 0f, 2f))
    }

    @Test
    fun a_pen_input_makes_a_palm_window_of_half_a_second() {
        val pen = dev.penwise.ink.PenActivity()
        pen.lastInputAt = 10_000
        assertTrue(pen.recent(10_400))
        assertFalse(pen.recent(10_600))
    }
}
