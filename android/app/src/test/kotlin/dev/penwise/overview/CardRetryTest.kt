package dev.penwise.overview

import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class CardRetryTest {
    private val waits = ArrayList<Long>()

    private fun run(
        tries: Int,
        replies: List<CardLoading.Attempt<String>>,
    ): String? {
        var next = 0
        return runBlocking {
            CardLoading.untilReady(tries, { waits.add(it) }) { replies[minOf(next++, replies.lastIndex)] }
        }
    }

    @Test
    fun a_503_then_a_200_fills_the_card_after_the_retry_after() {
        val got = run(5, listOf(CardLoading.Attempt.Busy(2_000), CardLoading.Attempt.Ready("png")))
        assertEquals("png", got)
        assertEquals(listOf(2_000L), waits)
    }

    @Test
    fun a_server_that_stays_busy_is_asked_a_limited_number_of_times() {
        assertNull(run(3, listOf(CardLoading.Attempt.Busy(500))))
        assertEquals(listOf(500L, 500L), waits)
    }

    @Test
    fun a_failure_is_not_retried() {
        assertNull(run(5, listOf(CardLoading.Attempt.Failed)))
        assertEquals(emptyList<Long>(), waits)
    }
}
