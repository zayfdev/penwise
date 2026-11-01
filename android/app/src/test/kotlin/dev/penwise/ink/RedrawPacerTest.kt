package dev.penwise.ink

import org.junit.Assert.assertEquals
import org.junit.Test
import java.util.concurrent.ConcurrentLinkedQueue
import java.util.concurrent.atomic.AtomicInteger

class RedrawPacerTest {
    private val frames = ArrayDeque<() -> Unit>()
    private var forwarded = 0
    private val pacer = RedrawPacer({ frames.addLast(it) }, { forwarded++ })

    private fun tick() = frames.removeFirstOrNull()?.invoke()

    @Test
    fun a_burst_between_frames_becomes_one_redraw_at_the_next_frame() {
        repeat(100) { pacer.request() }
        assertEquals(0, forwarded)
        assertEquals(1, frames.size)
        tick()
        assertEquals(1, forwarded)
        assertEquals(0, frames.size)
    }

    @Test
    fun a_request_after_a_frame_is_not_dropped() {
        pacer.request()
        tick()
        pacer.request()
        pacer.request()
        tick()
        assertEquals(2, forwarded)
    }

    @Test
    fun a_request_made_while_forwarding_schedules_another_frame() {
        var again = true
        lateinit var p: RedrawPacer
        p =
            RedrawPacer({ frames.addLast(it) }) {
                forwarded++
                if (again) {
                    again = false
                    p.request()
                }
            }
        p.request()
        tick()
        tick()
        assertEquals(2, forwarded)
    }

    @Test
    fun one_frame_per_scroll_frame_at_most() {
        repeat(5) {
            repeat(3) { pacer.request() }
            tick()
        }
        assertEquals(5, forwarded)
    }

    @Test
    fun a_request_from_a_thread_without_a_looper_is_forwarded_at_the_frame() {
        val queue = ConcurrentLinkedQueue<() -> Unit>()
        val seen = AtomicInteger()
        val shared = RedrawPacer({ queue.add(it) }) { seen.incrementAndGet() }
        val worker = Thread { repeat(10) { shared.request() } }
        worker.start()
        worker.join()
        queue.poll()?.invoke()
        assertEquals(1, seen.get())
    }

    @Test
    fun a_booking_that_fails_does_not_wedge_later_requests() {
        var failing = true
        val flaky =
            RedrawPacer({
                if (failing) error("no looper")
                frames.addLast(it)
            }) { forwarded++ }
        runCatching { flaky.request() }
        failing = false
        flaky.request()
        tick()
        assertEquals(1, forwarded)
    }
}
