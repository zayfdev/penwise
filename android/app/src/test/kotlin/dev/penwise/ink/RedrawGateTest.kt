package dev.penwise.ink

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** One page redraw in flight: a camera flight's per-frame requests must not take all of the renderer's buffers. */
class RedrawGateTest {
    private var sent = 0
    private var accepting = true
    private val gate =
        RedrawGate {
            if (accepting) sent++
            accepting
        }

    @Test
    fun requests_while_one_is_in_flight_collapse_into_one_sent_after_it_lands() {
        repeat(30) { gate.request() }
        assertEquals(1, sent)
        assertTrue(gate.landed())
        gate.request()
        assertEquals(2, sent)
        assertFalse(gate.landed())
    }

    @Test
    fun a_request_no_renderer_took_does_not_block_the_next() {
        accepting = false
        gate.request()
        accepting = true
        gate.request()
        assertEquals(1, sent)
    }

    @Test
    fun a_reset_forgets_the_redraw_the_old_surface_will_never_land() {
        gate.request()
        gate.request()
        gate.reset()
        assertFalse(gate.landed())
        gate.request()
        assertEquals(2, sent)
    }
}
