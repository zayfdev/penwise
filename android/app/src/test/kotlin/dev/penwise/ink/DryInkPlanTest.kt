package dev.penwise.ink

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** The in-window ink is recorded once and moved by the camera's transform; it is recorded again only when needed. */
class DryInkPlanTest {
    private val w = 2560
    private val h = 1600
    private val view = ViewTransform(1.7778f, 0f, -2000f)

    @Test
    fun a_scroll_inside_the_band_only_moves_the_recording() {
        val recorded = RecordedInk(view, DryInkPlan.band(view, w, h))
        val scrolled = view.copy(ty = view.ty - 900f)
        assertFalse(DryInkPlan.needsRecord(recorded, scrolled, w, h, sceneChanged = false, moving = false))
        val shift = DryInkPlan.shift(view, scrolled)
        assertEquals(1f, shift.scale, 0f)
        assertEquals(0f, shift.dx, 0f)
        assertEquals("the ink moves exactly as far as the camera", -900f, shift.dy, 0f)
    }

    @Test
    fun leaving_the_band_or_changing_the_scene_records_again() {
        val recorded = RecordedInk(view, DryInkPlan.band(view, w, h))
        assertTrue(DryInkPlan.needsRecord(recorded, view.copy(ty = view.ty - h - 1f), w, h, false, moving = true))
        assertTrue(DryInkPlan.needsRecord(recorded, view, w, h, sceneChanged = true, moving = false))
        assertTrue(DryInkPlan.needsRecord(null, view, w, h, false, false))
    }

    @Test
    fun a_zoom_is_drawn_scaled_while_it_moves_and_recorded_once_it_rests() {
        val recorded = RecordedInk(view, DryInkPlan.band(view, w, h))
        val zoomed = ViewTransform(view.scale * 1.25f, view.tx, view.ty)
        assertFalse(DryInkPlan.needsRecord(recorded, zoomed, w, h, false, moving = true))
        assertTrue(DryInkPlan.needsRecord(recorded, zoomed, w, h, false, moving = false))
        assertEquals(1.25f, DryInkPlan.shift(view, zoomed).scale, 1e-6f)
    }
}
