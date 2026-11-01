package dev.penwise.frames

import dev.penwise.ink.ViewTransform
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** Frame geometry: page rect + canvas view → screen placement, containment and the inverse finger mapping. */
class FrameGeometryTest {
    private val rect = FrameRect(x = 100f, y = 200f, width = 800f, height = 1700f)

    @Test
    fun an_unzoomed_unpanned_canvas_places_a_frame_where_the_page_has_it() {
        val placement = FrameGeometry.place(rect, ViewTransform.IDENTITY, pxPerCss = 1f)
        assertEquals(100f, placement.left, EPS)
        assertEquals(200f, placement.top, EPS)
        assertEquals(800f, placement.width, EPS)
        assertEquals(1700f, placement.height, EPS)
        assertEquals(1f, placement.scale, EPS)
    }

    @Test
    fun a_zoomed_and_panned_canvas_moves_the_corner_and_scales_the_page() {
        val placement = FrameGeometry.place(rect, ViewTransform(scale = 0.5f, tx = -40f, ty = 60f), pxPerCss = 1f)
        assertEquals(10f, placement.left, EPS)
        assertEquals(160f, placement.top, EPS)
        assertEquals(400f, placement.width, EPS)
        assertEquals(850f, placement.height, EPS)
        assertEquals(0.5f, placement.scale, EPS)
    }

    @Test
    fun a_window_point_maps_into_the_pages_own_pixels() {
        val placement = FrameGeometry.place(rect, ViewTransform(scale = 2f, tx = 10f, ty = 20f), pxPerCss = 1f)
        assertEquals(210f, placement.left, EPS)
        assertEquals(420f, placement.top, EPS)
        assertEquals(25f, placement.toLocalX(260f), EPS)
        assertEquals(40f, placement.toLocalY(500f), EPS)
    }

    @Test
    fun window_and_local_points_round_trip_at_several_zooms() {
        for (scale in listOf(0.5f, 1f, 1.75f, 4f)) {
            val placement = FrameGeometry.place(rect, ViewTransform(scale, tx = 13f, ty = -7f), pxPerCss = 1f)
            for (local in listOf(0f, 1f, 120.5f, 799f)) {
                val window = placement.left + local * placement.scale
                assertEquals(local, placement.toLocalX(window), TOL)
            }
        }
    }

    @Test
    fun the_frame_rect_is_half_open_so_adjacent_frames_never_both_claim_a_point() {
        val placement = FrameGeometry.place(rect, ViewTransform.IDENTITY, pxPerCss = 1f)
        assertTrue(placement.contains(100f, 200f))
        assertTrue(placement.contains(899.5f, 1899.5f))
        assertFalse(placement.contains(900f, 500f))
        assertFalse(placement.contains(500f, 1900f))
        assertFalse(placement.contains(99.5f, 500f))
    }

    @Test
    fun overlapping_frames_resolve_to_the_first_and_empty_space_to_minus_one() {
        val placements =
            FrameGeometry.placeAll(
                listOf(FrameRect(0f, 0f, 100f, 100f), FrameRect(50f, 0f, 100f, 100f)),
                ViewTransform.IDENTITY,
                pxPerCss = 1f,
            )
        assertEquals(0, FrameGeometry.at(60f, 60f, placements))
        assertEquals(1, FrameGeometry.at(120f, 60f, placements))
        assertEquals(-1, FrameGeometry.at(400f, 60f, placements))
    }

    @Test
    fun a_zoomed_out_frame_claims_a_smaller_screen_area() {
        val placement = FrameGeometry.place(rect, ViewTransform(scale = 0.25f, tx = 0f, ty = 0f), pxPerCss = 1f)
        assertEquals(25f, placement.left, EPS)
        assertEquals(50f, placement.top, EPS)
        assertEquals(200f, placement.width, EPS)
        assertEquals(425f, placement.height, EPS)
        assertEquals(0, FrameGeometry.at(30f, 60f, listOf(placement)))
        assertEquals(-1, FrameGeometry.at(20f, 40f, listOf(placement)))
    }

    @Test
    fun the_item_box_is_page_units_while_the_page_inside_is_laid_out_at_device_px() {
        // A phone preset at density 2: the box on the canvas is 390 page units wide (C1), the WebView inside is
        // 780 device px so its CSS viewport is 390 px; its layout px therefore take half a screen px each.
        val box = FrameRect(x = 0f, y = 0f, width = 390f, height = 844f)
        val placement = FrameGeometry.place(box, ViewTransform.IDENTITY, pxPerCss = 2f)
        assertEquals(390f, placement.width, EPS)
        assertEquals(844f, placement.height, EPS)
        assertEquals(0.5f, placement.scale, EPS)
        assertEquals(780f, placement.toLocalX(placement.right), EPS)
    }

    private companion object {
        const val EPS = 1e-4f
        const val TOL = 1e-2f
    }
}
