package dev.penwise.ink

import dev.penwise.ink.GridPinch.Outcome
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** The overview pinch's cancel path, its ratio and the rubber band the camera shows meanwhile. */
class GridPinchCancelTest {
    private val pinch = GridPinch()

    private fun pinchTo(
        ratio: Float,
        steps: Int = 10,
    ) {
        val factor = Math.pow(ratio.toDouble(), 1.0 / steps).toFloat()
        repeat(steps) { pinch.step(factor) }
    }

    @Test
    fun a_cancelled_pinch_never_opens_the_overview() {
        pinch.begin(atFitScale = true)
        pinchTo(0.5f)
        pinch.cancel()
        assertEquals(Outcome.NONE, pinch.end())
        pinch.begin(atFitScale = true)
        pinchTo(0.78f)
        pinch.cancel()
        assertEquals(Outcome.NONE, pinch.end())
    }

    @Test
    fun the_camera_follows_a_spread_by_the_pinch_ratio_and_ignores_the_dead_zone() {
        pinch.begin(atFitScale = true)
        pinchTo(0.9f)
        assertEquals(1f, pinch.effective, 1e-4f)
        pinchTo(1.2f)
        assertEquals(0.9f * 1.2f, pinch.effective, 1e-3f)
        pinch.begin(atFitScale = false)
        assertEquals(1f, pinch.effective, 0f)
    }

    @Test
    fun the_ratio_stays_within_the_zoom_range() {
        pinch.begin(atFitScale = true, zoomRatio = 4f)
        pinchTo(100f)
        assertEquals(4f, pinch.ratio, 1e-4f)
        pinchTo(0.5f)
        assertEquals(2f, pinch.ratio, 1e-3f)
    }

    @Test
    fun the_rubber_band_gives_a_little_and_never_more_than_the_floor() {
        assertEquals(1f, RubberBand.dip(1f), 0f)
        assertEquals(1f, RubberBand.dip(1.5f), 0f)
        assertEquals(0.91f, RubberBand.dip(0.82f), 1e-4f)
        assertEquals(RubberBand.FLOOR, RubberBand.dip(0.1f), 0f)
        assertTrue(RubberBand.dip(0.9f) > RubberBand.dip(0.8f))
    }

    @Test
    fun scaling_about_a_point_keeps_that_point_and_inverts() {
        val view = ViewTransform(2f, -300f, -500f)
        val dipped = RubberBand.about(view, 0.8f, 1000f, 600f)
        assertEquals(view.toPageX(1000f), dipped.toPageX(1000f), 1e-3f)
        assertEquals(view.toPageY(600f), dipped.toPageY(600f), 1e-3f)
        assertEquals(1.6f, dipped.scale, 1e-4f)
        val back = RubberBand.about(dipped, 1f / 0.8f, 1000f, 600f)
        assertEquals(view.tx, back.tx, 1e-2f)
        assertEquals(view.ty, back.ty, 1e-2f)
    }
}
