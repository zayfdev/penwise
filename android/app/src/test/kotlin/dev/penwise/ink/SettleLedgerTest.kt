package dev.penwise.ink

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** The background render has to land after a pan or zoom even when the scene changed meanwhile. */
class SettleLedgerTest {
    private val style = PageStyle("blank", 1000f, 1000f, false)
    private val panned = ViewTransform(2f, -300f, -120f)

    private fun region(
        left: Float,
        top: Float,
        right: Float,
        bottom: Float,
    ) = RenderPlan.Region(Bounds(left, top, right, bottom))

    @Test
    fun a_scene_change_after_a_pan_does_not_discard_the_settle_render() {
        val ledger = SettleLedger()
        ledger.submitted(panned, style)
        ledger.sceneChanged(region(10f, 20f, 30f, 40f), everything = false)
        assertTrue("the open render stays: no restart, so no starvation", ledger.runs(panned, style))
        val patch = ledger.accept(panned, panned, style)
        assertNotNull("the finished render is accepted", patch)
        assertEquals(10f, patch!!.left, 0f)
        assertEquals(40f, patch.bottom, 0f)
    }

    @Test
    fun changes_since_the_snapshot_are_merged_into_one_repaint_area() {
        val ledger = SettleLedger()
        ledger.submitted(panned, style)
        ledger.sceneChanged(region(10f, 20f, 30f, 40f), everything = false)
        ledger.sceneChanged(region(100f, 5f, 120f, 15f), everything = false)
        val patch = ledger.accept(panned, panned, style)!!
        assertEquals(10f, patch.left, 0f)
        assertEquals(5f, patch.top, 0f)
        assertEquals(120f, patch.right, 0f)
        assertEquals(40f, patch.bottom, 0f)
    }

    @Test
    fun a_render_nothing_changed_under_needs_no_repaint() {
        val ledger = SettleLedger()
        ledger.submitted(panned, style)
        assertTrue(ledger.accept(panned, panned, style)!!.isEmpty)
    }

    @Test
    fun a_change_before_the_request_is_not_counted_against_it() {
        val ledger = SettleLedger()
        ledger.sceneChanged(region(0f, 0f, 999f, 999f), everything = false)
        ledger.submitted(panned, style)
        assertTrue(ledger.accept(panned, panned, style)!!.isEmpty)
    }

    @Test
    fun an_unlocatable_change_voids_the_render() {
        val ledger = SettleLedger()
        ledger.submitted(panned, style)
        ledger.sceneChanged(RenderPlan.Idle, everything = true)
        assertFalse(ledger.runs(panned, style))
        assertNull(ledger.accept(panned, panned, style))
    }

    /**
     * A finger scroll pans the camera every frame, so a background render always finishes for a view the camera has
     * already left. Dropping it (as before) meant no fresh render landed during a scroll: strokes that came into view
     * stayed cut at the old screen edge until the scroll stopped (tablet, 2026-11-02: 3706 red px shown mid-scroll
     * instead of about 30 000). At the same zoom it is shown through the transform, like the bitmap it replaces.
     */
    @Test
    fun a_render_for_an_earlier_pan_lands_while_the_camera_scrolls_on() {
        val ledger = SettleLedger()
        ledger.submitted(panned, style)
        ledger.sceneChanged(region(10f, 20f, 30f, 40f), everything = false)
        val patch = ledger.accept(panned, ViewTransform(2f, -300f, -460f), style)
        assertNotNull("a render one scroll step behind is still the freshest the layer has", patch)
        assertEquals(20f, patch!!.top, 0f)
    }

    @Test
    fun a_render_for_another_zoom_or_style_is_dropped() {
        val ledger = SettleLedger()
        ledger.submitted(panned, style)
        assertNull(ledger.accept(panned, ViewTransform(3f, -300f, -120f), style))
        ledger.submitted(panned, style)
        assertNull(ledger.accept(panned, panned, style.copy(background = "grid")))
        assertFalse(ledger.runs(ViewTransform(1f, 0f, 0f), style))
    }

    @Test
    fun a_cleared_ledger_accepts_nothing() {
        val ledger = SettleLedger()
        ledger.submitted(panned, style)
        ledger.clear()
        assertNull(ledger.accept(panned, panned, style))
        assertFalse(ledger.runs(panned, style))
    }
}
