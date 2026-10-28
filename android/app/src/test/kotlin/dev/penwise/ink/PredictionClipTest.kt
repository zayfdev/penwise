package dev.penwise.ink

import org.junit.Assert.assertEquals
import org.junit.Test

class PredictionClipTest {
    private fun points(vararg xy: Float): FloatArray {
        val out = FloatArray(xy.size / 2 * 5)
        for (i in 0 until xy.size / 2) {
            out[i * 5] = xy[2 * i]
            out[i * 5 + 1] = xy[2 * i + 1]
        }
        return out
    }

    @Test
    fun a_tail_along_the_heading_is_kept() {
        val tail = points(2f, 0f, 4f, 0f)
        assertEquals(2, PredictionClip.clip(tail, 2, 0f, 0f, 1f, 0f, 10f))
        assertEquals(4f, tail[5], 0f)
    }

    @Test
    fun a_point_beyond_the_limit_is_pulled_back_and_ends_the_tail() {
        val tail = points(6f, 0f, 12f, 0f, 18f, 0f)
        assertEquals(2, PredictionClip.clip(tail, 3, 0f, 0f, 1f, 0f, 8f))
        assertEquals(6f, tail[0], 0f)
        assertEquals(8f, tail[5], 1e-4f)
    }

    @Test
    fun a_point_off_the_heading_ends_the_tail_before_it() {
        val tail = points(3f, 0f, 3f, 6f)
        assertEquals(1, PredictionClip.clip(tail, 2, 0f, 0f, 1f, 0f, 20f))
    }

    @Test
    fun a_pen_with_no_heading_predicts_nothing() {
        val tail = points(3f, 0f)
        assertEquals(0, PredictionClip.clip(tail, 1, 0f, 0f, 0f, 0f, 20f))
    }
}
