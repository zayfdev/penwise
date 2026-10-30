package dev.penwise.frames

import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** Send waits for the outbox, but never longer than three seconds. */
class OutboxWaitTest {
    private var now = 0L
    private val pauses = ArrayList<Long>()

    private fun wait(pending: () -> Int): Boolean =
        runBlocking {
            OutboxWait.untilAcked(pending, clock = { now }, pause = {
                pauses += it
                now += it
            })
        }

    @Test
    fun an_empty_outbox_does_not_wait_at_all() {
        assertTrue(wait { 0 })
        assertTrue(pauses.isEmpty())
    }

    @Test
    fun it_waits_until_the_last_op_is_acknowledged() {
        var left = 3
        assertTrue(wait { if (left > 0) left-- else 0 })
        assertEquals(3, pauses.size)
        assertTrue("well under the limit", now < OutboxWait.MAX_MS)
    }

    @Test
    fun a_server_that_never_acknowledges_is_waited_for_three_seconds_and_then_sent_to_anyway() {
        assertFalse(wait { 2 })
        assertEquals(OutboxWait.MAX_MS, now)
    }
}
