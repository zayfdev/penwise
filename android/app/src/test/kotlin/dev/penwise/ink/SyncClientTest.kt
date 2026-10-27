package dev.penwise.ink

import okhttp3.Request
import okhttp3.WebSocket
import okio.ByteString
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** The sync client under failure: shutting it down, and a full queue. */
class SyncClientTest {
    private val socket =
        object : WebSocket {
            override fun request(): Request = Request.Builder().url("http://localhost/").build()

            override fun queueSize(): Long = 0

            override fun send(text: String): Boolean = true

            override fun send(bytes: ByteString): Boolean = true

            override fun close(
                code: Int,
                reason: String?,
            ): Boolean = true

            override fun cancel() = Unit
        }

    private fun stroke(seed: Long) = SavedStroke(0, -0x1000000, 1f, seed, FloatArray(10), intArrayOf(0, 1000), 2)

    @Test
    fun late_okhttp_callbacks_after_stop_do_not_throw() {
        val client = SyncClient(emptyList())
        client.open = true
        client.stop()
        // OkHttp reports the close we trigger on its own threads; this used to hit a shut-down executor.
        client.listener.onClosed(socket, 1000, "reconfigured")
        client.listener.onFailure(socket, java.io.IOException("gone"), null)
        client.listener.onMessage(socket, """{"t":"meta"}""")
    }

    @Test
    fun previews_are_not_queued_while_disconnected() {
        val client = SyncClient(emptyList())
        client.sendStroke(stroke(1))
        repeat(PREVIEWS) { client.sendErasePreview(it.toFloat(), 0f, 5f) }
        client.sendLiveBegin(0, -0x1000000, 1f, 7L)
        val queued = client.queuedFrames()
        assertEquals(1, queued.size)
        assertTrue(queued[0].contains("\"stroke\""))
    }

    @Test
    fun a_flood_of_previews_never_evicts_a_queued_stroke() {
        val client = SyncClient(emptyList())
        client.sendStroke(stroke(1))
        client.sendClear()
        client.open = true // connected, but nothing drains the queue (no socket)
        repeat(PREVIEWS) { client.sendErasePreview(it.toFloat(), 0f, 5f) }
        val queued = client.queuedFrames()
        assertTrue("the stroke survived", queued.any { it.contains("\"stroke\"") })
        assertTrue("the clear survived", queued.any { it.contains("\"clear\"") })
        assertTrue("previews stay bounded", queued.size <= 2 + MAX_PREVIEWS)
    }

    private companion object {
        const val PREVIEWS = 300
        const val MAX_PREVIEWS = 64
    }
}
