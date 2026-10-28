package dev.penwise.ink

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

private const val LAN = "ws://192.168.1.19:7420/sync"
private const val TAILSCALE = "ws://100.64.10.100:7420/sync"

/** Which servers sync tries, in which order, and what a typed address or a pairing code does to the settings. */
class SyncSettingsTest {
    private val candidates = listOf(LAN, TAILSCALE)

    @Test
    fun the_override_comes_first_and_the_built_ins_follow() {
        // `adb reverse` address: once the cable is gone it must fall through to the LAN, then Tailscale.
        assertEquals(
            listOf("ws://127.0.0.1:7420/sync", LAN, TAILSCALE),
            endpointList("http://127.0.0.1:7420", candidates),
        )
    }

    @Test
    fun without_an_override_the_built_ins_are_used() {
        assertEquals(candidates, endpointList(null, candidates))
    }

    @Test
    fun an_override_equal_to_a_built_in_is_not_dialled_twice() {
        assertEquals(candidates, endpointList("http://192.168.1.19:7420", candidates))
    }

    @Test
    fun the_connected_endpoint_is_tried_first_for_asset_fetches() {
        assertEquals(listOf(TAILSCALE, LAN), connectedFirst(TAILSCALE, candidates))
        assertEquals(candidates, connectedFirst(null, candidates))
        assertEquals(candidates, connectedFirst("ws://elsewhere/sync", candidates))
    }

    @Test
    fun a_typed_address_without_a_scheme_still_builds_a_url() {
        assertEquals("http://192.168.1.19:7420", LibraryClient.httpBase("192.168.1.19:7420"))
        assertEquals("http://192.168.1.19:7420", LibraryClient.httpBase(" 192.168.1.19:7420/ "))
        assertEquals("http://192.168.1.19:7420", LibraryClient.httpBase("ws://192.168.1.19:7420/sync"))
        assertEquals("https://penwise.example", LibraryClient.httpBase("wss://penwise.example/sync?x=1"))
        assertEquals("http://host:7420", LibraryClient.httpBase("http://host:7420"))
        // The point: every result parses as a URL.
        listOf("192.168.1.19:7420", "host", "ws://h/sync", "").forEach {
            java.net.URL(LibraryClient.httpBase(it))
        }
    }

    @Test
    fun a_failed_pairing_changes_nothing() {
        val update = syncUpdateFor("http://host:7420", "ABC123") { _, _ -> null }
        assertNull("the code must not become the token", update)
    }

    @Test
    fun a_good_pairing_saves_the_servers_token_and_address() {
        val update =
            syncUpdateFor(null, "abc-123") { url, code ->
                assertNull(url)
                assertEquals("ABC123", code)
                "the-real-token" to "http://192.168.1.19:7420"
            }!!
        assertEquals("the-real-token", update.token)
        assertEquals("http://192.168.1.19:7420", update.url)
    }

    @Test
    fun a_pairing_without_any_address_keeps_the_saved_one() {
        val update = syncUpdateFor(null, "ABC123") { _, _ -> "tok" to "" }!!
        assertNull(update.url)
    }

    @Test
    fun a_plain_token_is_saved_as_typed_and_an_unedited_address_stays() {
        val update = syncUpdateFor(null, "a".repeat(43)) { _, _ -> error("not a code") }!!
        assertNull(update.url)
        assertEquals("a".repeat(43), update.token)
        assertFalse(LibraryClient.looksLikeCode(update.token))
        assertTrue(true)
    }
}
