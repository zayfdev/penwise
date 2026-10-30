package dev.penwise.frames

import com.sun.net.httpserver.HttpServer
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import java.net.InetSocketAddress

/** The still route as the client sees it: fresh, still being made (`503` + `Retry-After`), and the backoff. */
class StillClientTest {
    private val server = HttpServer.create(InetSocketAddress("127.0.0.1", 0), 0)
    private var status = 200
    private var retryAfter: String? = null
    private val body =
        """{"version":3,"doc":[1440,900],"viewport":[1440,900],"strips":[],"layout":{"rects":{},"state":""}}"""

    init {
        server.createContext("/frames/f/still") { exchange ->
            retryAfter?.let { exchange.responseHeaders.add("Retry-After", it) }
            exchange.responseHeaders.add("ETag", "\"v3\"")
            val bytes = if (status == 200) body.toByteArray() else ByteArray(0)
            exchange.sendResponseHeaders(status, if (bytes.isEmpty()) -1 else bytes.size.toLong())
            if (bytes.isNotEmpty()) exchange.responseBody.use { it.write(bytes) }
            exchange.close()
        }
        server.start()
    }

    @After
    fun stop() = server.stop(0)

    private val client by lazy { StillClient(FrameServer("http://127.0.0.1:${server.address.port}", "")) }

    @Test
    fun a_fresh_still_carries_its_manifest_and_etag() {
        val reply = client.manifest("f", null) as StillReply.Fresh
        assertEquals("3", reply.manifest.version)
        assertEquals("\"v3\"", reply.etag)
    }

    @Test
    fun a_503_with_retry_after_says_when_to_ask_again() {
        status = 503
        retryAfter = "2"
        val reply = client.manifest("f", null) as StillReply.Busy
        assertEquals(2_000L, reply.retryAfterMs)
    }

    @Test
    fun a_503_without_a_usable_header_backs_off_for_the_default() {
        status = 503
        assertEquals(StillBackoff.DEFAULT_MS, (client.manifest("f", null) as StillReply.Busy).retryAfterMs)
        assertEquals(StillBackoff.DEFAULT_MS, StillBackoff.retryAfterMs("soon"))
        assertEquals(30_000L, StillBackoff.retryAfterMs("9999"))
    }

    @Test
    fun a_frame_in_backoff_is_left_alone_until_the_time_is_up() {
        val backoff = StillBackoff()
        backoff.busy("f", now = 10_000L, retryAfterMs = 2_000L)
        assertTrue(backoff.blocked("f", 11_999L))
        assertFalse(backoff.blocked("f", 12_000L))
        assertFalse("another frame is not held back", backoff.blocked("g", 10_500L))
        backoff.busy("f", 20_000L, 2_000L)
        backoff.clear("f")
        assertFalse(backoff.blocked("f", 20_100L))
    }
}
