package dev.penwise.ink

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** A committed stroke leaves the front layer when the window shows it, also with strokes in quick succession. */
class FrontHandoffTest {
    @Test
    fun a_lone_stroke_clears_the_front_buffer_once_its_hide_landed() {
        val handoff = FrontHandoff()
        handoff.committed()
        assertTrue(handoff.pending)
        assertEquals(FrontHandoff.Landed.CLEAR, handoff.landed())
        assertFalse(handoff.pending)
    }

    @Test
    fun a_stroke_started_before_the_hide_landed_shows_the_layer_again() {
        val handoff = FrontHandoff()
        handoff.committed()
        handoff.frontDrawn()
        assertEquals(FrontHandoff.Landed.SHOW, handoff.landed())
    }

    @Test
    fun only_the_last_of_several_hides_decides() {
        val handoff = FrontHandoff()
        handoff.committed()
        handoff.frontDrawn()
        handoff.committed()
        assertEquals(FrontHandoff.Landed.WAIT, handoff.landed())
        assertEquals("the second stroke is in the window too", FrontHandoff.Landed.CLEAR, handoff.landed())
        assertEquals(FrontHandoff.Landed.WAIT, handoff.landed())
    }

    @Test
    fun an_abandoned_gesture_leaves_nothing_to_show() {
        val handoff = FrontHandoff()
        handoff.committed()
        handoff.frontDrawn()
        handoff.abandoned()
        assertEquals(FrontHandoff.Landed.CLEAR, handoff.landed())
    }
}
