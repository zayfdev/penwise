package dev.penwise.ink

import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** Live preview frames are batched per stroke: nothing lost, nothing queued behind. */
class LiveBatcherTest {
    private fun sample(x: Float) = floatArrayOf(x, 0f, 0.5f, 0f, 0f)

    @Test
    fun batches_of_one_stroke_are_merged_in_order() {
        val batcher = LiveBatcher()
        batcher.add(1, sample(10f), intArrayOf(0), 1)
        batcher.add(1, sample(20f), intArrayOf(8), 1)
        assertTrue(batcher.pending)
        val samples = FloatArray(10)
        val times = IntArray(2)
        assertEquals(2, batcher.take(samples, times))
        assertEquals(10f, samples[0], 0f)
        assertEquals(20f, samples[5], 0f)
        assertArrayEquals(intArrayOf(0, 8), times)
        assertFalse("taking empties the batch", batcher.pending)
    }

    @Test
    fun a_new_stroke_drops_the_previous_batch() {
        val batcher = LiveBatcher()
        batcher.add(1, sample(10f), intArrayOf(0), 1)
        batcher.add(2, sample(90f), intArrayOf(5), 1)
        val samples = FloatArray(5)
        val times = IntArray(1)
        assertEquals("only the new stroke", 1, batcher.take(samples, times))
        assertEquals(90f, samples[0], 0f)
    }

    @Test
    fun reset_discards_everything() {
        val batcher = LiveBatcher()
        batcher.add(1, sample(10f), intArrayOf(0), 1)
        batcher.reset()
        assertFalse(batcher.pending)
        assertEquals(0, batcher.take(FloatArray(5), IntArray(1)))
    }

    @Test
    fun many_samples_grow_past_the_initial_capacity() {
        val batcher = LiveBatcher()
        val chunk = FloatArray(5 * 100) { it.toFloat() }
        val chunkTimes = IntArray(100) { it }
        repeat(5) { batcher.add(1, chunk, chunkTimes, 100) }
        assertEquals(500, batcher.take(FloatArray(5 * 500), IntArray(500)))
    }
}
