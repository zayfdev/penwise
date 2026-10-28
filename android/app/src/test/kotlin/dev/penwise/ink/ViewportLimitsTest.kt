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
        assertTrue("page edge stays near the screen", far.tx <= 256f + EPS && far.ty <= 160f + EPS)
        state.transformBy(4f, 1280f, 800f, 1280f, 800f)
        state.transformBy(1f, 0f, 0f, -100_000f, -100_000f)
        val view = state.transform
        assertTrue("right edge stays on screen", view.tx + 2560f * view.scale >= 2560f * 0.9f - TOL)
        assertTrue("bottom edge stays on screen", view.ty + 1600f * view.scale >= 1600f * 0.9f - TOL)
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
