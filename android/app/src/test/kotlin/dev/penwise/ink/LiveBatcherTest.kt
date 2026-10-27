package dev.penwise.ink

import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** Live preview samples are merged per tick, chunked, and indexed so the receiver can place each chunk. */
class LiveBatcherTest {
    private fun sample(x: Float) = floatArrayOf(x, 0f, 0.5f, 0f, 0f)

    @Test
    fun batches_are_merged_in_order_and_advance_the_start_index() {
        val batcher = LiveBatcher()
        batcher.add(sample(10f), intArrayOf(0), 1)
        batcher.add(sample(20f), intArrayOf(8), 1)
        assertTrue(batcher.pending)
        assertEquals(0, batcher.start)
        val samples = FloatArray(10)
        val times = IntArray(2)
        assertEquals(2, batcher.take(samples, times, max = 2))
        assertEquals(10f, samples[0], 0f)
        assertEquals(20f, samples[5], 0f)
        assertArrayEquals(intArrayOf(0, 8), times)
        assertFalse("taking empties the batch", batcher.pending)
        assertEquals(2, batcher.start)
    }

    @Test
    fun a_chunk_limit_leaves_the_rest_for_the_next_frame() {
        val batcher = LiveBatcher()
        repeat(3) { batcher.add(sample(it.toFloat()), intArrayOf(it), 1) }
        val samples = FloatArray(10)
        val times = IntArray(2)
        assertEquals(2, batcher.take(samples, times, max = 2))
        assertEquals(1, batcher.take(samples, times, max = 2))
        assertEquals(2f, samples[0], 0f)
        assertEquals(3, batcher.start)
    }

    @Test
    fun reset_starts_a_new_stroke_at_index_zero() {
        val batcher = LiveBatcher()
        batcher.add(sample(1f), intArrayOf(0), 1)
        batcher.take(FloatArray(5), IntArray(1), max = 1)
        batcher.reset()
        assertEquals(0, batcher.start)
        assertFalse(batcher.pending)
    }
}
