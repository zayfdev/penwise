package dev.penwise.ink

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** The camera locked to a frame's box (contract §3.1): min scale, clamped pinch/pan/fit, the fit-scale flag. */
class ViewportLockTest {
    private val infinite = PageStyle("blank", 0f, 0f, infinite = true)

    // A 1440 x 4000 box at (2000, 100) on a 2880 x 1600 screen: fit-width scale 2.
    private val box = PageRect(2000f, 100f, 1440f, 4000f)

    private fun locked(): ViewportState =
        ViewportState().apply {
            setPage(infinite)
            setViewSize(2880f, 1600f)
            lockTo(box)
        }

    @Test
    fun locking_lands_on_the_fit_width_view_of_the_box_top() {
        val view = locked().transform
        assertEquals(2f, view.scale, EPS)
        assertEquals(2000f, view.toPageX(0f), EPS)
        assertEquals(3440f, view.toPageX(2880f), EPS)
        assertEquals(100f, view.toPageY(0f), EPS)
    }

    @Test
    fun a_pinch_cannot_zoom_out_of_the_box() {
        val state = locked()
        state.transformBy(0.2f, 1440f, 800f, 1440f, 800f)
        assertEquals(2f, state.transform.scale, EPS)
        assertEquals(2000f, state.transform.toPageX(0f), EPS)
    }

    @Test
    fun a_pinch_in_zooms_up_to_max_scale_and_stays_in_the_box() {
        val state = locked()
        state.transformBy(100f, 2880f, 1600f, 2880f, 1600f)
        val view = state.transform
        assertEquals(ViewportState.MAX_SCALE, view.scale, EPS)
        assertTrue(view.toPageX(0f) >= 2000f - EPS && view.toPageX(2880f) <= 3440f + EPS)
        assertTrue(view.toPageY(0f) >= 100f - EPS && view.toPageY(1600f) <= 4100f + EPS)
    }

    @Test
    fun a_zoomed_in_view_pans_freely_but_not_past_the_box_edges() {
        val state = locked()
        state.transformBy(4f, 0f, 0f, 0f, 0f)
        state.transformBy(1f, 0f, 0f, 90_000f, 90_000f)
        assertEquals(2000f, state.transform.toPageX(0f), EPS)
        assertEquals(100f, state.transform.toPageY(0f), EPS)
        state.transformBy(1f, 0f, 0f, -90_000f, -90_000f)
        assertEquals(3440f, state.transform.toPageX(2880f), EPS)
        assertEquals(4100f, state.transform.toPageY(1600f), EPS)
    }

    @Test
    fun at_the_fit_scale_a_pan_moves_only_up_and_down() {
        val state = locked()
        state.panBy(500f, -300f)
        assertEquals(2000f, state.transform.toPageX(0f), EPS)
        assertEquals(100f + 150f, state.transform.toPageY(0f), EPS)
    }

    @Test
    fun a_scroll_link_pan_is_clamped_at_the_end_of_the_box() {
        val state = locked()
        // The page scrolled 10 000 CSS px under the finger: the camera pans by the same amount, stopping at the end.
        state.panBy(0f, -10_000f * 2f)
        assertEquals(4100f, state.transform.toPageY(1600f), EPS)
        // A scroll back is followed 1:1 from the clamped end.
        state.panBy(0f, 200f)
        assertEquals(4100f - 100f, state.transform.toPageY(1600f), EPS)
    }

    @Test
    fun a_fit_to_something_else_lands_inside_the_box() {
        val state = locked()
        state.fitRect(PageRect(-5000f, -5000f, 20_000f, 20_000f), FitMode.FIT_ALL)
        val view = state.transform
        assertEquals(2f, view.scale, EPS)
        assertTrue(view.toPageX(0f) >= 2000f - EPS && view.toPageX(2880f) <= 3440f + EPS)
        assertTrue(view.toPageY(0f) >= 100f - EPS)
    }

    @Test
    fun the_fit_scale_flag_is_true_only_at_the_minimum_scale() {
        val state = locked()
        assertTrue(state.atFitScale)
        state.transformBy(1.5f, 0f, 0f, 0f, 0f)
        assertFalse(state.atFitScale)
        state.transformBy(1f / 1.5f, 0f, 0f, 0f, 0f)
        assertTrue(state.atFitScale)
        assertFalse(ViewportState().atFitScale)
    }

    private companion object {
        const val EPS = 1e-2f
    }
}
