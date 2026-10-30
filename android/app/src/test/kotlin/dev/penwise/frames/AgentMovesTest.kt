package dev.penwise.frames

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** An outside camera move waits while the owner draws (contract §3). */
class AgentMovesTest {
    private val target = MoveTarget("page", "frame", "Work /")

    @Test
    fun a_move_within_ten_seconds_of_the_last_pen_stroke_is_held_for_the_chip() {
        val moves = AgentMoves()
        assertFalse(moves.admit(target, nowMs = 19_000L, lastDrawMs = 10_000L))
        assertEquals(target, moves.pending)
    }

    @Test
    fun a_move_after_ten_quiet_seconds_happens_at_once() {
        val moves = AgentMoves()
        assertTrue(moves.admit(target, nowMs = 20_000L, lastDrawMs = 10_000L))
        assertNull(moves.pending)
    }

    @Test
    fun a_pen_that_never_drew_never_defers() {
        assertTrue(AgentMoves().admit(target, nowMs = 100L, lastDrawMs = 0L))
    }

    @Test
    fun the_chip_does_the_held_move_once() {
        val moves = AgentMoves()
        moves.admit(target, nowMs = 12_000L, lastDrawMs = 10_000L)
        assertEquals(target, moves.take())
        assertNull(moves.take())
    }

    @Test
    fun a_newer_held_move_replaces_the_older_one_and_dismiss_drops_it() {
        val moves = AgentMoves()
        val other = MoveTarget("page", null, "Page 2")
        moves.admit(target, nowMs = 12_000L, lastDrawMs = 10_000L)
        moves.admit(other, nowMs = 13_000L, lastDrawMs = 10_000L)
        assertEquals(other, moves.pending)
        moves.dismiss()
        assertNull(moves.pending)
    }
}
