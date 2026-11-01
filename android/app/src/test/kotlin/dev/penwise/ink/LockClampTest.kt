package dev.penwise.ink

import org.junit.Assert.assertEquals
import org.junit.Test

/** The clamp maths of a locked camera: the screen stays inside one frame's box, zoom stops at the fit-width scale. */
class LockClampTest {
    // A 1000 x 3000 box at (500, 200) on a 2000 x 1200 screen: fit-width scale 2, so the box is 6000 tall on screen.
    private val box = PageRect(500f, 200f, 1000f, 3000f)

    private fun clamp(view: ViewTransform) = LockClamp.clamp(view, box, 2000f, 1200f, 8f)

    @Test
    fun the_minimum_scale_is_the_boxs_fit_width_scale() {
        assertEquals(2f, LockClamp.minScale(box, 2000f, 8f), EPS)
    }

    @Test
    fun a_box_narrower_than_an_eighth_of_the_screen_still_zooms_to_max_scale() {
        assertEquals(8f, LockClamp.minScale(PageRect(0f, 0f, 100f, 100f), 2000f, 8f), EPS)
    }

    @Test
    fun zooming_out_is_stopped_at_the_fit_width_scale() {
        val view = clamp(ViewTransform(0.5f, 0f, 0f))
        assertEquals(2f, view.scale, EPS)
    }

    @Test
    fun at_the_fit_scale_the_screen_cannot_move_sideways() {
        val right = clamp(ViewTransform(2f, 300f, -400f))
        val left = clamp(ViewTransform(2f, -900f, -400f))
        assertEquals(-1000f, right.tx, EPS)
        assertEquals(-1000f, left.tx, EPS)
    }

    @Test
    fun zoomed_in_the_screen_stays_inside_the_box_on_both_sides() {
        // Scale 4: the box is 4000 wide on a 2000 screen, so tx runs from 2000 - 6000 to -2000.
        assertEquals(-2000f, clamp(ViewTransform(4f, 100f, -900f)).tx, EPS)
        assertEquals(-4000f, clamp(ViewTransform(4f, -9000f, -900f)).tx, EPS)
        assertEquals(-3000f, clamp(ViewTransform(4f, -3000f, -900f)).tx, EPS)
    }

    @Test
    fun the_screen_stays_between_the_boxs_top_and_bottom() {
        // Scale 2: the box spans 400..6400 on screen at ty = 0, so ty runs from 1200 - 6400 to -400.
        assertEquals(-400f, clamp(ViewTransform(2f, -1000f, 500f)).ty, EPS)
        assertEquals(-5200f, clamp(ViewTransform(2f, -1000f, -99_000f)).ty, EPS)
        assertEquals(-1234f, clamp(ViewTransform(2f, -1000f, -1234f)).ty, EPS)
    }

    @Test
    fun a_box_shorter_than_the_screen_is_centred() {
        val short = PageRect(0f, 100f, 1000f, 400f)
        val view = LockClamp.clamp(ViewTransform(2f, 0f, 0f), short, 2000f, 1200f, 8f)
        // The box is 800 tall on screen: 200 above and below.
        assertEquals(200f, view.toScreenY(100f), EPS)
        assertEquals(1000f, view.toScreenY(500f), EPS)
    }

    @Test
    fun a_clamped_view_never_shows_board_outside_the_box() {
        for (scale in listOf(0.1f, 2f, 3f, 8f)) {
            for (shift in listOf(-50_000f, -1000f, 0f, 700f, 50_000f)) {
                val view = clamp(ViewTransform(scale, shift, shift))
                val s = view.scale
                assertEquals(true, view.toPageX(0f) >= 500f - EPS && view.toPageX(2000f) <= 1500f + EPS)
                assertEquals(true, view.toPageY(0f) >= 200f - EPS && view.toPageY(1200f) <= 3200f + EPS)
                assertEquals(true, s >= 2f - EPS)
            }
        }
    }

    @Test
    fun the_locked_view_starts_at_the_boxs_top_left_at_fit_width() {
        val view = LockClamp.fitTop(box, 2000f, 1200f, 8f)
        assertEquals(2f, view.scale, EPS)
        assertEquals(500f, view.toPageX(0f), EPS)
        assertEquals(1500f, view.toPageX(2000f), EPS)
        assertEquals(200f, view.toPageY(0f), EPS)
    }

    private companion object {
        const val EPS = 1e-2f
    }
}
