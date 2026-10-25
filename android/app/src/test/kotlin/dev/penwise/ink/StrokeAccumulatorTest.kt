package dev.penwise.ink

import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * Regression: strokes used to be persisted from the last motion event only, so every stroke saved a single
 * sample and every page reloaded as dots.
 */
class StrokeAccumulatorTest {
    private fun delta(
        x: Float,
        t: Int,
    ) = floatArrayOf(x, 10f, 0.5f, 0f, 0f) to intArrayOf(t)

    @Test
    fun every_delta_of_a_stroke_is_kept_in_order() {
        val accumulator = StrokeAccumulator()
        var x = 0f
        repeat(20) { step ->
            val (samples, times) = delta(x, step * 1_000)
            accumulator.add(samples, times, 1)
            x += 5f
        }
        assertEquals("all twenty samples, not just the last", 20, accumulator.count)
        assertEquals(20 * 5, accumulator.samplesCopy().size)
        assertEquals(0f, accumulator.samplesCopy()[0], 0f)
        assertEquals(95f, accumulator.samplesCopy()[19 * 5], 0f)
        assertArrayEquals(intArrayOf(0, 1_000, 2_000), accumulator.timesCopy().copyOf(3))
    }

    @Test
    fun a_delta_carrying_several_samples_is_appended_wholesale() {
        val accumulator = StrokeAccumulator()
        accumulator.add(floatArrayOf(1f, 1f, 0.5f, 0f, 0f, 2f, 2f, 0.5f, 0f, 0f), intArrayOf(0, 10), 2)
        accumulator.add(floatArrayOf(3f, 3f, 0.5f, 0f, 0f), intArrayOf(20), 1)
        assertEquals(3, accumulator.count)
        assertEquals(3f, accumulator.samplesCopy()[2 * 5], 0f)
    }

    @Test
    fun reset_starts_a_new_stroke() {
        val accumulator = StrokeAccumulator()
        accumulator.add(floatArrayOf(1f, 1f, 0.5f, 0f, 0f), intArrayOf(0), 1)
        accumulator.reset()
        assertEquals(0, accumulator.count)
        assertEquals(0, accumulator.samplesCopy().size)
        accumulator.add(floatArrayOf(9f, 9f, 0.5f, 0f, 0f), intArrayOf(5), 1)
        assertEquals(9f, accumulator.samplesCopy()[0], 0f)
    }

    @Test
    fun a_long_stroke_grows_past_the_initial_capacity() {
        val accumulator = StrokeAccumulator()
        repeat(1_000) { step ->
            accumulator.add(floatArrayOf(step.toFloat(), 0f, 0.5f, 0f, 0f), intArrayOf(step), 1)
        }
        assertEquals(1_000, accumulator.count)
        assertEquals(999f, accumulator.samplesCopy()[999 * 5], 0f)
    }

    @Test
    fun an_empty_delta_changes_nothing() {
        val accumulator = StrokeAccumulator()
        accumulator.add(floatArrayOf(), intArrayOf(), 0)
        assertEquals(0, accumulator.count)
    }
}
