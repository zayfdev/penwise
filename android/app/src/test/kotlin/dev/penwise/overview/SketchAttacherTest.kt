package dev.penwise.overview

import dev.penwise.frames.LaneKey
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** New sketch pages reach their strip: after the outbox ack, with retries on 404, kept while the server is away. */
class SketchAttacherTest {
    private val lane = LaneKey("p", "/", "desktop")
    private val events = ArrayList<String>()
    private val replies = ArrayDeque<AttachResult>()
    private var attached = 0

    // Unconfined and non-suspending fakes: a flush runs to its end inside the call that started it.
    private fun attacher() =
        SketchAttacher(
            scope = CoroutineScope(Dispatchers.Unconfined),
            send = { _, page ->
                events += "put $page"
                replies.removeFirstOrNull() ?: AttachResult.OK
            },
            outbox = { 0 },
            onAttached = { attached++ },
            io = Dispatchers.Unconfined,
            pause = { events += "pause $it" },
            untilAcked = { events += "ack" },
        )

    @Test
    fun the_put_waits_for_the_outbox_ack_first() {
        val attacher = attacher()
        attacher.add(lane, SKETCH_1)
        assertEquals(listOf("ack", "put $SKETCH_1"), events)
        assertTrue(attacher.pending.isEmpty())
        assertEquals(1, attached)
    }

    @Test
    fun a_404_is_retried_with_a_growing_pause_until_the_page_has_synced() {
        replies += listOf(AttachResult.NOT_FOUND, AttachResult.NOT_FOUND, AttachResult.OK)
        val attacher = attacher()
        attacher.add(lane, SKETCH_1)
        val put = "put $SKETCH_1"
        assertEquals(listOf("ack", put, "pause 500", put, "pause 1000", put), events)
        assertTrue(attacher.pending.isEmpty())
    }

    @Test
    fun a_page_the_server_never_finds_stays_pending_after_the_retries() {
        replies += List(10) { AttachResult.NOT_FOUND }
        val attacher = attacher()
        attacher.add(lane, SKETCH_1)
        assertEquals(4, events.count { it.startsWith("put") })
        assertEquals(mapOf(SKETCH_1 to lane), attacher.pending)
        assertEquals(0, attached)
    }

    @Test
    fun an_offline_server_keeps_the_page_and_the_next_flush_sends_it_again() {
        replies += AttachResult.FAILED
        val attacher = attacher()
        attacher.add(lane, SKETCH_1)
        assertEquals(mapOf(SKETCH_1 to lane), attacher.pending)
        attacher.flush()
        assertTrue(attacher.pending.isEmpty())
        assertEquals(2, events.count { it == "put $SKETCH_1" })
        assertEquals(1, attached)
    }

    @Test
    fun a_page_the_server_refuses_is_dropped_without_a_strip_refresh() {
        replies += AttachResult.REJECTED
        val attacher = attacher()
        attacher.add(lane, SKETCH_1)
        assertTrue(attacher.pending.isEmpty())
        assertEquals(0, attached)
    }

    @Test
    fun an_offline_server_stops_the_pass_so_later_pages_wait_too() {
        replies += listOf(AttachResult.FAILED, AttachResult.FAILED)
        val attacher = attacher()
        attacher.add(lane, SKETCH_1)
        attacher.add(lane, SKETCH_2)
        assertFalse(events.any { it == "put $SKETCH_2" })
        assertEquals(listOf(SKETCH_1, SKETCH_2), attacher.pending.keys.toList())
    }
}
