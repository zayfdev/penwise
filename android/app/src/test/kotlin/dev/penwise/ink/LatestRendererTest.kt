package dev.penwise.ink

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.util.concurrent.Executor

/** Only the newest requested render is ever published; superseded and cancelled ones are disposed. */
class LatestRendererTest {
    private val queue = ArrayDeque<Runnable>()
    private val disposed = ArrayList<String>()
    private var ready = 0
    private val executor = Executor { queue.addLast(it) }
    private val renderer =
        LatestRenderer<String, String>(executor, { request, _ -> "render:$request" }, { disposed += it }, { ready++ })

    private fun runAll() {
        while (queue.isNotEmpty()) queue.removeFirst().run()
    }

    @Test
    fun a_finished_current_render_is_taken_once() {
        renderer.submit("a")
        assertTrue(renderer.pending)
        runAll()
        assertEquals(1, ready)
        assertEquals("render:a", renderer.take())
        assertNull(renderer.take())
        assertFalse(renderer.pending)
    }

    @Test
    fun a_render_overtaken_by_a_newer_request_is_dropped() {
        renderer.submit("old")
        renderer.submit("new")
        runAll()
        assertEquals("render:new", renderer.take())
        assertEquals(listOf<String>(), disposed)
        assertEquals(1, ready)
    }

    @Test
    fun a_result_that_finished_before_a_newer_request_is_disposed_not_taken() {
        renderer.submit("old")
        queue.removeFirst().run()
        renderer.submit("new")
        assertEquals(listOf("render:old"), disposed)
        assertNull(renderer.take())
        runAll()
        assertEquals("render:new", renderer.take())
    }

    @Test
    fun cancel_drops_in_flight_and_finished_results() {
        renderer.submit("a")
        runAll()
        renderer.cancel()
        assertNull(renderer.take())
        assertEquals(listOf("render:a"), disposed)
        assertFalse(renderer.pending)
        renderer.submit("b")
        renderer.cancel()
        runAll()
        assertNull(renderer.take())
        assertEquals(1, ready)
    }

    @Test
    fun a_render_sees_that_it_was_superseded_while_running() {
        var seen = true
        lateinit var slow: LatestRenderer<String, String>
        slow =
            LatestRenderer(
                executor,
                { request, current ->
                    if (request == "first") slow.submit("second")
                    if (request == "first") seen = current()
                    "done:$request"
                },
                { disposed += it },
                { ready++ },
            )
        slow.submit("first")
        runAll()
        assertFalse(seen)
        assertEquals(listOf("done:first"), disposed)
        assertEquals("done:second", slow.take())
    }
}
