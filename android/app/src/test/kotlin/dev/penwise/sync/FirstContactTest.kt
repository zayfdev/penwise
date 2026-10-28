package dev.penwise.sync

import org.junit.Assert.assertEquals
import org.junit.Test

/** A first launch away from home must not decide "no server" (and seed an empty notebook) after one timeout. */
class FirstContactTest {
    private val urls = listOf("ws://lan/sync", "ws://tailscale/sync")
    private var reports = 0
    private val contact = FirstContact({ urls }) { reports++ }

    @Test
    fun one_failed_endpoint_is_not_unreachable() {
        contact.failed("ws://lan/sync")
        assertEquals(0, reports)
        contact.failed("ws://lan/sync")
        assertEquals("the same endpoint failing twice still leaves Tailscale untried", 0, reports)
    }

    @Test
    fun every_endpoint_failing_is_reported_once() {
        contact.failed("ws://lan/sync")
        contact.failed("ws://tailscale/sync")
        contact.failed("ws://tailscale/sync")
        assertEquals(1, reports)
    }

    @Test
    fun no_endpoints_at_all_is_unreachable() {
        FirstContact({ emptyList() }) { reports++ }.failed(null)
        assertEquals(1, reports)
    }

    @Test
    fun a_server_that_answered_is_never_unreachable() {
        contact.welcomed()
        contact.failed("ws://lan/sync")
        contact.failed("ws://tailscale/sync")
        assertEquals(0, reports)
    }
}
