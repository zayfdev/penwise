package dev.penwise.frames

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** The stills' poll runs between start and stop, and not in the background. */
class PollLoopTest {
    private val queue = ArrayList<Pair<Runnable, Long>>()
    private var runs = 0
    private val loop =
        PollLoop(3_000L, { job, delay -> queue += job to delay }, { job ->
            queue.removeAll {
                it.first ===
                    job
            }
        }) { runs++ }

    /** Runs what is queued, as the clock would. */
    private fun fire() {
        val due = queue.toList()
        queue.clear()
        due.forEach { it.first.run() }
    }

    @Test
    fun starting_posts_the_first_run_and_each_run_posts_the_next_after_the_period() {
        loop.start()
        assertTrue(loop.running)
        assertEquals(0, runs)
        fire()
        assertEquals(1, runs)
        assertEquals(listOf(3_000L), queue.map { it.second })
        fire()
        assertEquals(2, runs)
    }

    @Test
    fun starting_twice_does_not_double_the_poll() {
        loop.start()
        loop.start()
        assertEquals(1, queue.size)
    }

    @Test
    fun stopping_withdraws_the_next_run_and_nothing_runs_in_the_background() {
        loop.start()
        fire()
        loop.stop()
        assertFalse(loop.running)
        assertTrue(queue.isEmpty())
        fire()
        assertEquals(1, runs)
    }

    @Test
    fun a_run_that_was_already_due_when_the_loop_stopped_does_nothing() {
        loop.start()
        val due = queue.toList()
        loop.stop()
        due.forEach { it.first.run() }
        assertEquals(0, runs)
        assertTrue(queue.isEmpty())
    }

    @Test
    fun it_picks_up_again_when_started_after_a_stop() {
        loop.start()
        fire()
        loop.stop()
        loop.start()
        fire()
        assertEquals(2, runs)
    }
}
