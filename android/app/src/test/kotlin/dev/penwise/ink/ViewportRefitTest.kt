package dev.penwise.ink

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** A locked camera that showed the whole width keeps doing so when the screen or the box changes width. */
class ViewportRefitTest {
    private fun locked(): ViewportState =
        ViewportState().apply {
            setPage(PageStyle("blank", 0f, 0f, infinite = true))
            setViewSize(2880f, 1600f)
            lockTo(PageRect(2000f, 100f, 1440f, 4000f))
        }

    @Test
    fun a_new_screen_size_refits_a_camera_that_showed_the_whole_width() {
        val state = locked()
        state.panBy(0f, -500f)
        val top = state.transform.toPageY(0f)
        state.setViewSize(1440f, 2560f)
        assertEquals(1f, state.transform.scale, EPS)
        assertEquals(2000f, state.transform.toPageX(0f), EPS)
        assertEquals(3440f, state.transform.toPageX(1440f), EPS)
        assertEquals(top, state.transform.toPageY(0f), EPS)
    }

    @Test
    fun a_new_screen_size_only_clamps_a_camera_that_was_zoomed_in() {
        val state = locked()
        state.transformBy(1.5f, 0f, 0f, 0f, 0f)
        state.setViewSize(1440f, 2560f)
        assertEquals(3f, state.transform.scale, EPS)
        assertTrue(state.transform.toPageX(1440f) <= 3440f + EPS)
    }

    @Test
    fun a_preset_change_refits_the_new_width_from_the_same_page_y() {
        val state = locked()
        state.panBy(0f, -500f)
        val top = state.transform.toPageY(0f)
        state.relock(PageRect(2000f, 100f, 390f, 4000f))
        assertEquals(2880f / 390f, state.transform.scale, EPS)
        assertEquals(2000f, state.transform.toPageX(0f), EPS)
        assertEquals(top, state.transform.toPageY(0f), EPS)
    }

    @Test
    fun a_preset_change_leaves_a_zoomed_in_camera_inside_the_new_box() {
        val state = locked()
        state.transformBy(4f, 0f, 0f, 0f, 0f)
        state.relock(PageRect(2000f, 100f, 1000f, 4000f))
        assertTrue(state.transform.toPageX(2880f) <= 3000f + EPS)
    }

    private companion object {
        const val EPS = 1e-2f
    }
}
