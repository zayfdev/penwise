package dev.penwise.sync

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class NetworkRetryTest {
    private var now = 10_000L
    private val retry = NetworkRetry({ now }, debounceMs = 300, minGapMs = 2_000)

    @Test
    fun an_event_waits_the_debounce_so_a_burst_collapses_into_the_last_look() {
        assertEquals(300L, retry.onValidated("wifi"))
        now += 400
        assertEquals(300L, retry.onValidated("cell"))
        now += 400
        assertEquals(300L, retry.onValidated("wifi"))
        assertEquals("wifi", retry.latest)
    }

    @Test
    fun a_redial_pushes_the_next_one_out_by_the_minimum_gap() {
        retry.onValidated("wifi")
        retry.redialed()
        now += 500
        assertEquals(1_500L, retry.onValidated("cell"))
        now += 5_000
        assertEquals(300L, retry.onValidated("cell"))
    }

    @Test
    fun a_socket_waiting_out_its_backoff_is_redialled() {
        retry.onValidated("wifi")
        assertTrue(retry.shouldRedial(hasSocket = false, online = false, openedOn = null))
    }

    @Test
    fun a_dial_in_flight_is_never_aborted() {
        retry.onValidated("cell")
        assertFalse(retry.shouldRedial(hasSocket = true, online = false, openedOn = "wifi"))
    }

    @Test
    fun a_healthy_socket_is_kept_unless_the_network_changed() {
        retry.onValidated("wifi")
        retry.onValidated("cell")
        retry.onValidated("wifi")
        // wifi -> cell -> wifi inside a second: the socket opened on wifi is still right.
        assertFalse(retry.shouldRedial(hasSocket = true, online = true, openedOn = "wifi"))
        assertFalse("unknown origin is left alone", retry.shouldRedial(true, true, null))
        retry.onValidated("cell")
        assertTrue(retry.shouldRedial(hasSocket = true, online = true, openedOn = "wifi"))
    }
}
