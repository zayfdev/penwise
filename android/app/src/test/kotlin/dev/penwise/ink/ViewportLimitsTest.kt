package dev.penwise.ink

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** Zoom limits, the fixed-page clamp, free pan on infinite pages, reset and the gesture flag. */
class ViewportLimitsTest {
    private val fixed = PageStyle("blank", 2560f, 1600f, infinite = false)
    private val infinite = PageStyle("blank", 0f, 0f, infinite = true)

    private fun viewport(
        style: PageStyle,
        width: Float = 2560f,
        height: Float = 1600f,
    ) = ViewportState().apply {
        setPage(style)
        setViewSize(width, height)
    }

    @Test
    fun zoom_stays_within_its_limits() {
        val state = viewport(infinite)
        state.transformBy(1000f, 0f, 0f, 0f, 0f)
        assertEquals(ViewportState.MAX_SCALE, state.transform.scale, EPS)
        assertEquals(ViewportState.MAX_SCALE, state.zoom.value, EPS)
        state.transformBy(0.0001f, 0f, 0f, 0f, 0f)
        assertEquals(ViewportState.MIN_SCALE, state.transform.scale, EPS)
    }

    @Test
    fun a_fixed_page_cannot_be_flung_off_screen() {
        val state = viewport(fixed)
        state.transformBy(1f, 0f, 0f, 100_000f, 100_000f)
        val far = state.transform
        assertEquals(2560f - ViewportState.KEEP_PX, far.tx, EPS)
        assertEquals(1600f - ViewportState.KEEP_PX, far.ty, EPS)
        state.transformBy(4f, 1280f, 800f, 1280f, 800f)
        state.transformBy(1f, 0f, 0f, -100_000f, -100_000f)
        val view = state.transform
        assertEquals(ViewportState.KEEP_PX, view.tx + 2560f * view.scale, TOL)
        assertEquals(ViewportState.KEEP_PX, view.ty + 1600f * view.scale, TOL)
    }

    @Test
    fun the_top_left_corner_of_a_fixed_page_can_fill_the_screen_at_4x() {
        val state = viewport(fixed)
        state.transformBy(4f, 0f, 0f, 0f, 0f)
        // Pull the page right and down until its top-left corner sits in the screen's top-left corner.
        state.transformBy(1f, 0f, 0f, 2_000f, 1_000f)
        state.transformBy(1f, 2_000f, 1_000f, 0f, 0f)
        val view = state.transform
        assertEquals(4f, view.scale, EPS)
        assertEquals(0f, view.tx, TOL)
        assertEquals(0f, view.ty, TOL)
        assertEquals(0f, view.toPageX(0f), TOL)
        assertEquals(640f, view.toPageX(2560f), TOL)
    }

    @Test
    fun any_corner_of_a_fixed_page_can_be_brought_to_the_screen_centre() {
        val state = viewport(fixed)
        state.transformBy(4f, 1280f, 800f, 1280f, 800f)
        state.transformBy(1f, 0f, 0f, -100_000f, -100_000f)
        val view = state.transform
        // The bottom-right of the page is the last thing visible, so it can be dragged back to mid-screen.
        state.transformBy(1f, view.toScreenX(2560f), view.toScreenY(1600f), 1280f, 800f)
        assertEquals(1280f, state.transform.toScreenX(2560f), TOL)
        assertEquals(800f, state.transform.toScreenY(1600f), TOL)
    }

    @Test
    fun an_infinite_page_pans_freely() {
        val state = viewport(infinite)
        state.transformBy(1f, 0f, 0f, 100_000f, -100_000f)
        assertEquals(100_000f, state.transform.tx, EPS)
        assertEquals(-100_000f, state.transform.ty, EPS)
    }

    @Test
    fun reset_fits_the_page_again() {
        val state = viewport(fixed)
        state.transformBy(3f, 500f, 500f, 700f, 650f)
        assertTrue(state.zoom.value > 1f)
        state.reset()
        assertEquals(ViewTransform(1f, 0f, 0f), state.transform)
        assertEquals(1f, state.zoom.value, EPS)
    }

    @Test
    fun switching_to_another_sheet_fits_it() {
        val state = viewport(fixed)
        state.transformBy(3f, 500f, 500f, 700f, 650f)
        state.setPage(infinite)
        assertEquals(ViewTransform.IDENTITY, state.transform)
    }

    @Test
    fun a_view_at_the_fitted_position_is_refitted_when_the_screen_resizes() {
        val state = viewport(fixed)
        state.setViewSize(1280f, 800f)
        assertEquals(0.5f, state.transform.scale, EPS)
    }

    @Test
    fun the_gesturing_flag_notifies_only_when_it_changes() {
        val state = viewport(fixed)
        var calls = 0
        state.onChanged = { calls++ }
        state.setGesturing(true)
        state.setGesturing(true)
        assertTrue(state.gesturing)
        state.setGesturing(false)
        assertFalse(state.gesturing)
        assertEquals(2, calls)
    }

    private companion object {
        const val EPS = 1e-4f
        const val TOL = 1e-2f
    }
}
