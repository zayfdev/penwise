package dev.penwise.overview

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

/** A card shows what it has and then what the server has now; opening never waits on a hung server. */
class CardLoadingTest {
    private val background = CoroutineScope(SupervisorJob() + Dispatchers.IO)

    @Test
    fun the_picture_in_memory_shows_first_and_the_fetched_one_replaces_it() {
        val shown = ArrayList<String>()
        runBlocking { CardLoading.showThenRefresh(peek = { "old" }, load = { "new" }, show = { shown += it }) }
        assertEquals(listOf("old", "new"), shown)
    }

    @Test
    fun without_a_picture_in_memory_only_the_fetched_one_shows() {
        val shown = ArrayList<String>()
        runBlocking { CardLoading.showThenRefresh(peek = { null }, load = { "new" }, show = { shown += it }) }
        assertEquals(listOf("new"), shown)
    }

    @Test
    fun a_failed_fetch_keeps_the_picture_in_memory() {
        val shown = ArrayList<String>()
        runBlocking { CardLoading.showThenRefresh(peek = { "old" }, load = { null }, show = { shown += it }) }
        assertEquals(listOf("old"), shown)
    }

    @Test
    fun a_fetch_that_blocks_returns_the_fallback_within_the_time_limit() {
        val release = CountDownLatch(1)
        val started = System.nanoTime()
        val result =
            runBlocking {
                // A blocking socket read: a plain thread block that no coroutine timeout can interrupt.
                CardLoading.within(background, 300L, { "stale" }) {
                    release.await(10, TimeUnit.SECONDS)
                    "late"
                }
            }
        val tookMs = (System.nanoTime() - started) / 1_000_000
        release.countDown()
        assertEquals("stale", result)
        assertTrue("took $tookMs ms", tookMs < 2_000)
    }

    @Test
    fun a_fetch_that_blocks_with_nothing_in_memory_gives_null() {
        val release = CountDownLatch(1)
        val result =
            runBlocking {
                CardLoading.within<String>(background, 200L, { null }) {
                    release.await(10, TimeUnit.SECONDS)
                    "late"
                }
            }
        release.countDown()
        assertNull(result)
    }

    @Test
    fun a_fast_fetch_wins_over_the_picture_in_memory() {
        val result = runBlocking { CardLoading.within(background, 1_000L, { "stale" }) { "fresh" } }
        assertEquals("fresh", result)
    }
}
