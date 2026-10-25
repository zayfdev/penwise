package dev.penwise.ink

import org.junit.Assert.assertEquals
import org.junit.Test

/** Sync dials a WebSocket, but the viewer URL (and the pairing QR) is http; both must work. */
class WebSocketUrlTest {
    @Test
    fun a_viewer_url_becomes_the_socket_url() {
        assertEquals("ws://192.168.1.19:7420/ws", toWebSocketUrl("http://192.168.1.19:7420"))
        assertEquals("ws://192.168.1.19:7420/ws", toWebSocketUrl("http://192.168.1.19:7420/"))
        assertEquals("ws://192.168.1.19:7420/ws", toWebSocketUrl("http://192.168.1.19:7420/?token=abc"))
    }

    @Test
    fun a_socket_url_is_left_alone() {
        assertEquals("ws://192.168.1.19:7420/ws", toWebSocketUrl("ws://192.168.1.19:7420/ws"))
        assertEquals("wss://penwise.example/ws", toWebSocketUrl("wss://penwise.example/ws"))
    }

    @Test
    fun https_becomes_wss() {
        assertEquals("wss://penwise.example/ws", toWebSocketUrl("https://penwise.example"))
    }

    @Test
    fun a_bare_host_gets_a_scheme() {
        assertEquals("ws://192.168.1.19:7420/ws", toWebSocketUrl("192.168.1.19:7420"))
        assertEquals("", toWebSocketUrl("   "))
    }
}
