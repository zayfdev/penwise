package dev.penwise.ink

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** Shape tool geometry: bounds, closure and sample plumbing (pressure/tilt/timing). */
class ShapeSamplesTest {
    private fun points(
        mode: ShapeMode,
        x0: Float,
        y0: Float,
        x1: Float,
        y1: Float,
    ): FloatArray =
        assertNotNull(ShapeSamples.build(mode, x0, y0, x1, y1)).let { built ->
            ShapeSamples.build(mode, x0, y0, x1, y1)!!.first
        }

    @Test
    fun rectangle_stays_inside_the_drag_box_and_closes() {
        val samples = points(ShapeMode.Rect, 100f, 200f, 300f, 400f)
        val n = samples.size / 5
        var minX = Float.MAX_VALUE
        var maxX = -Float.MAX_VALUE
        var minY = Float.MAX_VALUE
        var maxY = -Float.MAX_VALUE
        for (i in 0 until n) {
            val x = samples[i * 5]
            val y = samples[i * 5 + 1]
            minX = minOf(minX, x)
            maxX = maxOf(maxX, x)
            minY = minOf(minY, y)
            maxY = maxOf(maxY, y)
            assertEquals(ShapeSamples.PRESSURE, samples[i * 5 + 2], 1e-6f)
        }
        assertEquals(100f, minX, 1e-3f)
        assertEquals(300f, maxX, 1e-3f)
        assertEquals(200f, minY, 1e-3f)
        assertEquals(400f, maxY, 1e-3f)
        assertEquals("closes back to the start", minX, samples[(n - 1) * 5], 1e-3f)
        assertEquals(minY, samples[(n - 1) * 5 + 1], 1e-3f)
    }

    @Test
    fun ellipse_points_lie_on_the_ellipse() {
        val samples = points(ShapeMode.Ellipse, 0f, 0f, 200f, 100f)
        val n = samples.size / 5
        val cx = 100f
        val cy = 50f
        for (i in 0 until n) {
            val dx = (samples[i * 5] - cx) / 100f
            val dy = (samples[i * 5 + 1] - cy) / 50f
            assertEquals("on the ellipse", 1.0, (dx * dx + dy * dy).toDouble(), 0.02)
        }
    }

    @Test
    fun line_is_two_points_and_arrow_adds_barbs() {
        val line = points(ShapeMode.Line, 0f, 0f, 100f, 100f).size / 5
        val arrow = points(ShapeMode.Arrow, 0f, 0f, 100f, 100f).size / 5
        assertEquals("a line is its two ends (the engine walks the segment)", 2, line)
        assertTrue("arrow adds barbs", arrow > line)
    }

    @Test
    fun descent_into_a_degenerate_drag_still_returns_samples() {
        val built = ShapeSamples.build(ShapeMode.Line, 50f, 50f, 50f, 50f)
        assertNotNull(built)
        assertEquals("one repeated end point", 2, built!!.first.size / 5)
    }

    @Test
    fun times_start_at_zero_and_increase() {
        val (_, times) = ShapeSamples.build(ShapeMode.Rect, 0f, 0f, 80f, 40f)!!
        assertEquals(0, times[0])
        for (i in 1 until times.size) assertTrue(times[i] > times[i - 1])
    }
}
