package dev.penwise.sync

import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Test

/** Which endpoint the reconnect loop dials next. */
class BackoffTest {
    private val urls = listOf("ws://usb/sync", "ws://lan/sync", "ws://tailscale/sync")
    private val thread = SyncThread()
    private val backoff = Backoff()

    @After
    fun stop() = thread.shutdown()

    private fun fail() {
        backoff.schedule(thread) {}.cancel(false)
    }

    @Test
    fun failures_walk_the_list_from_the_first_endpoint() {
        assertEquals(0, backoff.endpoint(urls))
        fail()
        assertEquals(1, backoff.endpoint(urls))
        fail()
        assertEquals(2, backoff.endpoint(urls))
        fail()
        assertEquals(0, backoff.endpoint(urls))
    }

    @Test
    fun a_drop_after_a_good_connection_retries_that_endpoint_first() {
        fail() // usb unplugged: falls through to the LAN
        backoff.welcomed(urls[1])
        fail() // the server restarts: LAN again, not Tailscale
        assertEquals(1, backoff.endpoint(urls))
        fail() // still gone: only now move on
        assertEquals(2, backoff.endpoint(urls))
    }

    @Test
    fun a_restart_begins_at_the_last_good_endpoint_unless_the_settings_changed() {
        backoff.welcomed(urls[2])
        fail()
        fail()
        backoff.restart()
        assertEquals(2, backoff.endpoint(urls))
        backoff.restart(forgetGood = true)
        assertEquals(0, backoff.endpoint(urls))
    }

    @Test
    fun a_good_endpoint_that_left_the_list_is_forgotten() {
        backoff.welcomed("ws://gone/sync")
        assertEquals(0, backoff.endpoint(urls))
    }
}
