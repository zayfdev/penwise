package dev.penwise.ink

import android.view.MotionEvent
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** The two-finger gestures on a locked camera: the rubber band, the overview pinch and what a cancel does. */
class CameraGesturesTest {
    private var opened = 0
    private val host =
        object : FrameHost {
            override fun route(
                event: MotionEvent,
                handOver: (MotionEvent) -> Unit,
            ) = false

            override fun cancel() = Unit

            override fun openOverview() {
                opened++
            }
        }
    private val viewport =
        ViewportState().apply {
            setPage(PageStyle("blank", 0f, 0f, infinite = true))
            setViewSize(2880f, 1600f)
            lockTo(PageRect(2000f, 100f, 1440f, 4000f))
        }
    private val gestures = CameraGestures({ viewport }, { host })

    private fun pinch(ratio: Float) {
        val steps = 10
        val factor = Math.pow(ratio.toDouble(), 1.0 / steps).toFloat()
        repeat(steps) { gestures.step(NavStep(factor, 1440f, 800f, 1440f, 800f)) }
    }

    @Test
    fun a_pinch_out_at_fit_shrinks_the_frame_a_little_and_springs_back_on_release() {
        gestures.active(true)
        pinch(0.9f)
        val scale = viewport.transform.scale
        assertTrue("dipped: $scale", scale < 2f && scale >= 2f * RubberBand.FLOOR)
        gestures.active(false)
        assertEquals(2f, viewport.transform.scale, EPS)
        assertEquals(0, opened)
    }

    @Test
    fun a_pinch_to_0_8_opens_the_overview_on_release_and_the_camera_returns_to_fit() {
        gestures.active(true)
        pinch(0.8f)
        assertEquals(0, opened)
        gestures.active(false)
        assertEquals(1, opened)
        assertEquals(2f, viewport.transform.scale, EPS)
    }

    @Test
    fun a_pinch_past_0_7_opens_the_overview_at_once_and_only_once() {
        gestures.active(true)
        pinch(0.6f)
        assertEquals(1, opened)
        gestures.active(false)
        assertEquals(1, opened)
        assertEquals(2f, viewport.transform.scale, EPS)
    }

    @Test
    fun a_cancel_after_a_deep_pinch_opens_nothing_and_returns_to_fit() {
        gestures.active(true)
        pinch(0.78f)
        gestures.abort()
        gestures.active(false)
        assertEquals(0, opened)
        assertEquals(2f, viewport.transform.scale, EPS)
    }

    @Test
    fun a_pinch_out_followed_by_a_spread_ends_at_the_pinch_ratio_not_beyond() {
        gestures.active(true)
        pinch(0.9f)
        pinch(1.2f)
        gestures.active(false)
        assertEquals(2f * 0.9f * 1.2f, viewport.transform.scale, 1e-2f)
        assertEquals(0, opened)
    }

    @Test
    fun a_pinch_that_starts_zoomed_in_is_plain_zoom_without_overview_or_band() {
        viewport.transformBy(1.5f, 1440f, 800f, 1440f, 800f)
        val start = viewport.transform.scale
        gestures.active(true)
        pinch(0.9f)
        assertEquals(start * 0.9f, viewport.transform.scale, 1e-2f)
        pinch(0.3f)
        gestures.active(false)
        assertEquals(0, opened)
        assertEquals(2f, viewport.transform.scale, EPS)
    }

    private companion object {
        const val EPS = 1e-2f
    }
}
