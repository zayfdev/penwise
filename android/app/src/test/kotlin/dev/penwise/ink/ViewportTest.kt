package dev.penwise.ink

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** The page → screen transform: fit, mapping, pinch about a focal point, limits and the fixed-page clamp. */
class ViewportTest {
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
    fun fitting_a_page_the_size_of_the_screen_is_one_to_one() {
        assertEquals(ViewTransform(1f, 0f, 0f), viewport(fixed).transform)
    }

    @Test
    fun fitting_a_page_into_a_smaller_screen_scales_it_down_and_centres_it() {
        val view = viewport(fixed, 1280f, 1000f).transform
        assertEquals(0.5f, view.scale, EPS)
        assertEquals(0f, view.tx, EPS)
        assertEquals(100f, view.ty, EPS)
    }

    @Test
    fun fitting_into_a_wider_screen_centres_horizontally() {
        val view = viewport(fixed, 3000f, 1600f).transform
        assertEquals(1f, view.scale, EPS)
        assertEquals(220f, view.tx, EPS)
    }

    @Test
    fun an_infinite_page_fits_to_one_to_one_from_its_origin() {
        assertEquals(ViewTransform.IDENTITY, viewport(infinite, 1000f, 700f).transform)
    }

    @Test
    fun screen_to_page_and_back_round_trips_at_several_zooms_and_pans() {
        val state = viewport(infinite)
        val points = listOf(0f to 0f, 100f to 250f, 2559f to 1599f, -40f to 3000f)
        for ((factor, dx) in listOf(2f to 80f, 1.5f to -300f, 0.5f to 40f, 3f to 500f)) {
            state.transformBy(factor, 1000f, 800f, 1000f + dx, 800f - dx / 2)
            val view = state.transform
            for ((x, y) in points) {
                assertEquals(x, view.toPageX(view.toScreenX(x)), TOL)
                assertEquals(y, view.toPageY(view.toScreenY(y)), TOL)
                assertEquals(x, view.toScreenX(view.toPageX(x)), TOL)
                assertEquals(y, view.toScreenY(view.toPageY(y)), TOL)
            }
        }
    }

    @Test
    fun pinching_keeps_the_page_point_under_the_focal_point_fixed() {
        val state = viewport(infinite)
        state.transformBy(1f, 0f, 0f, 120f, -60f)
        val focalX = 900f
        val focalY = 500f
        val pageX = state.transform.toPageX(focalX)
        val pageY = state.transform.toPageY(focalY)
        state.transformBy(2.5f, focalX, focalY, focalX, focalY)
        assertEquals(2.5f, state.transform.scale, EPS)
        assertEquals(focalX, state.transform.toScreenX(pageX), TOL)
        assertEquals(focalY, state.transform.toScreenY(pageY), TOL)
    }

    @Test
    fun a_pinch_that_also_drags_moves_the_point_to_the_new_focal_point() {
        val state = viewport(infinite)
        val pageX = state.transform.toPageX(400f)
        val pageY = state.transform.toPageY(300f)
        state.transformBy(2f, 400f, 300f, 700f, 350f)
        assertEquals(700f, state.transform.toScreenX(pageX), TOL)
        assertEquals(350f, state.transform.toScreenY(pageY), TOL)
    }

    private companion object {
        const val EPS = 1e-4f
        const val TOL = 1e-2f
    }
}
