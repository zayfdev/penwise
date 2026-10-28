package dev.penwise.frames

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** Touch routing: the pen always draws, one finger on a page is the page's, everything else is the canvas's. */
class FrameInputTest {
    /** A phone frame on screen, from a canvas at 1:1 and 100 px in. */
    private val phone = FramePlacement(left = 300f, top = 200f, width = 390f, height = 844f, scale = 1f)
    private val frames = listOf(phone)

    @Test
    fun the_stylus_always_draws_even_over_a_page() {
        assertEquals(FrameTarget.INK, FrameTouch.target(true, fingers = 1, x = 400f, y = 400f, placements = frames))
        assertEquals(FrameTarget.INK, FrameTouch.target(true, fingers = 2, x = 400f, y = 400f, placements = frames))
    }

    @Test
    fun one_finger_on_a_page_belongs_to_the_page() {
        assertEquals(FrameTarget.FRAME, FrameTouch.target(false, fingers = 1, x = 400f, y = 400f, placements = frames))
    }

    @Test
    fun one_finger_outside_every_page_belongs_to_nobody() {
        assertEquals(FrameTarget.NONE, FrameTouch.target(false, fingers = 1, x = 40f, y = 40f, placements = frames))
        val nowhere = FrameTouch.target(false, fingers = 1, x = 40f, y = 40f, placements = emptyList())
        assertEquals(FrameTarget.NONE, nowhere)
    }

    @Test
    fun two_fingers_are_always_the_canvas_even_over_a_page() {
        assertEquals(FrameTarget.CANVAS, FrameTouch.target(false, fingers = 2, x = 400f, y = 400f, placements = frames))
        assertEquals(FrameTarget.CANVAS, FrameTouch.target(false, fingers = 3, x = 40f, y = 40f, placements = frames))
    }

    @Test
    fun the_page_gets_the_sequence_only_while_one_finger_is_down() {
        assertTrue(FrameTouch.keepsFrame(1))
        assertFalse(FrameTouch.keepsFrame(2))
        assertFalse(FrameTouch.keepsFrame(3))
    }

    @Test
    fun a_zoomed_out_page_claims_the_screen_area_it_now_covers() {
        val small = phone.copy(left = 0f, top = 0f, width = 195f, height = 422f, scale = 0.5f)
        assertEquals(FrameTarget.FRAME, FrameTouch.target(false, 1, 100f, 100f, listOf(small)))
        assertEquals(FrameTarget.NONE, FrameTouch.target(false, 1, 200f, 100f, listOf(small)))
    }
}
