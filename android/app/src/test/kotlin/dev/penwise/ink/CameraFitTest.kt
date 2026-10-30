package dev.penwise.ink

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** The camera's fit maths and its 220 ms flight. */
class CameraFitTest {
    private val screen = CameraScreen(2560f, 1600f, minScale = 0.05f, maxScale = 8f)

    @Test
    fun fit_width_puts_a_1440_wide_frame_edge_to_edge_on_a_2560_screen() {
        val frame = PageRect(100f, 50f, 1440f, 900f)
        val target = CameraFit.target(frame, FitMode.FIT_WIDTH, screen, ViewTransform.IDENTITY)
        assertEquals(2560f / 1440f, target.scale, EPS)
        assertEquals(0f, target.toScreenX(100f), EPS)
        assertEquals(2560f, target.toScreenX(100f + 1440f), EPS)
        // The 16:10 desktop frame also fills the height.
        assertEquals(0f, target.toScreenY(50f), EPS)
        assertEquals(1600f, target.toScreenY(950f), EPS)
    }

    @Test
    fun fit_width_keeps_the_top_of_the_part_that_was_visible() {
        val frame = PageRect(0f, 0f, 1440f, 6000f)
        // The camera looks at document y 2000 of the frame.
        val current = ViewTransform(1f, 0f, -2000f)
        val target = CameraFit.target(frame, FitMode.FIT_WIDTH, screen, current)
        assertEquals(2000f, target.toPageY(0f), EPS)
        assertEquals(2560f / 1440f, target.scale, EPS)
    }

    @Test
    fun fit_width_of_a_frame_off_screen_starts_at_its_top() {
        val frame = PageRect(5000f, 3000f, 1440f, 6000f)
        val target = CameraFit.target(frame, FitMode.FIT_WIDTH, screen, ViewTransform.IDENTITY)
        assertEquals(3000f, target.toPageY(0f), EPS)
        assertEquals(5000f, target.toPageX(0f), EPS)
    }

    @Test
    fun fit_width_never_scrolls_past_the_end_of_the_frame() {
        val frame = PageRect(0f, 0f, 1440f, 2000f)
        val current = ViewTransform(1f, 0f, -1900f)
        val target = CameraFit.target(frame, FitMode.FIT_WIDTH, screen, current)
        assertEquals(2000f, target.toPageY(1600f), EPS)
    }

    @Test
    fun fit_all_shows_the_whole_rect_centred() {
        val frame = PageRect(0f, 0f, 1440f, 10_000f)
        val target = CameraFit.target(frame, FitMode.FIT_ALL, screen, ViewTransform.IDENTITY)
        assertEquals(0.16f, target.scale, EPS)
        assertEquals(0f, target.toScreenY(0f), EPS)
        assertEquals(1600f, target.toScreenY(10_000f), EPS)
        assertEquals(1280f, target.toScreenX(720f), EPS)
    }

    @Test
    fun the_flight_starts_where_it_was_ends_exactly_on_target_and_eases_out() {
        val from = ViewTransform(0.5f, 100f, 200f)
        val to = ViewTransform(2f, -300f, -900f)
        val tween = CameraTween(from, to, 2560f, 1600f)
        assertEquals(from.scale, tween.at(0).scale, EPS)
        assertEquals(to, tween.at(CameraTween.DURATION_MS))
        // Ease-out: well over half of the way after half of the time.
        val half = tween.at(CameraTween.DURATION_MS / 2)
        assertTrue(half.scale > 1.2f)
        assertTrue(CameraTween.easeOutCubic(0.5f) > 0.8f)
    }

    @Test
    fun fit_rect_flies_through_the_driver_and_a_touch_cancels_it() {
        val state = ViewportState()
        state.setPage(PageStyle("blank", 0f, 0f, infinite = true))
        state.setViewSize(2560f, 1600f)
        var requests = 0
        state.driver = CameraDriver { requests++ }
        state.fitRect(PageRect(0f, 0f, 1440f, 900f), FitMode.FIT_WIDTH)
        assertEquals(1, requests)
        assertTrue(state.animating)
        assertTrue(state.flight.step(1_000))
        assertTrue(state.flight.step(1_100))
        val mid = state.transform.scale
        assertTrue(mid > 1f && mid < 2560f / 1440f)
        state.flight.cancel()
        assertFalse(state.animating)
        assertFalse(state.flight.step(1_300))
        assertEquals(mid, state.transform.scale, EPS)
    }

    @Test
    fun fit_rect_lands_exactly_on_the_target() {
        val state = ViewportState()
        state.setPage(PageStyle("blank", 0f, 0f, infinite = true))
        state.setViewSize(2560f, 1600f)
        state.driver = CameraDriver { }
        state.fitRect(PageRect(0f, 0f, 1440f, 900f), FitMode.FIT_WIDTH)
        state.flight.step(5_000)
        assertFalse(state.flight.step(5_000 + CameraTween.DURATION_MS))
        assertEquals(2560f / 1440f, state.transform.scale, EPS)
        assertEquals(0f, state.transform.tx, EPS)
    }

    @Test
    fun without_a_driver_the_camera_jumps() {
        val state = ViewportState()
        state.setPage(PageStyle("blank", 0f, 0f, infinite = true))
        state.setViewSize(2560f, 1600f)
        state.fitRect(PageRect(0f, 0f, 1440f, 900f), FitMode.FIT_WIDTH)
        assertEquals(2560f / 1440f, state.transform.scale, EPS)
    }

    @Test
    fun an_infinite_page_zooms_out_to_five_percent_for_a_very_long_frame() {
        val state = ViewportState()
        state.setPage(PageStyle("blank", 0f, 0f, infinite = true))
        state.setViewSize(2560f, 1600f)
        state.fitRect(PageRect(0f, 0f, 1440f, 40_000f), FitMode.FIT_ALL)
        assertEquals(ViewportState.MIN_SCALE_INFINITE, state.transform.scale, EPS)
    }

    private companion object {
        const val EPS = 1e-3f
    }
}
