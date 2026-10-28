package dev.penwise.ink

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** Two-finger drag and pinch: what navigation reports, and that it never ends as a tap. */
class FingerPanZoomTest {
    private var taps = 0
    private val steps = ArrayList<NavStep>()
    private val active = ArrayList<Boolean>()
    private val nav = FingerNavigation(SLOP, { taps++ }, { steps += it }, { active += it })

    private fun twoDown() {
        nav.down(0, 400f, 500f, 0)
        nav.down(1, 600f, 500f, 20)
    }

    private fun moveTo(
        x0: Float,
        y0: Float,
        x1: Float,
        y1: Float,
    ) {
        nav.move(0, x0, y0)
        nav.move(1, x1, y1)
        nav.update()
    }

    @Test
    fun a_drag_pans_and_never_taps_even_if_the_fingers_return_and_lift_quickly() {
        twoDown()
        moveTo(500f, 520f, 700f, 520f)
        moveTo(400f, 500f, 600f, 500f)
        nav.up(0, 120)
        nav.up(1, 130)
        assertEquals(0, taps)
        assertTrue(steps.isNotEmpty())
        assertEquals(listOf(true, false), active)
    }

    @Test
    fun the_first_step_starts_from_where_the_fingers_went_down() {
        twoDown()
        moveTo(500f, 520f, 700f, 520f)
        val first = steps.first()
        assertEquals(500f, first.fromX, 1e-3f)
        assertEquals(500f, first.fromY, 1e-3f)
        assertEquals(600f, first.toX, 1e-3f)
        assertEquals(520f, first.toY, 1e-3f)
        assertEquals(1f, first.factor, 1e-3f)
    }

    @Test
    fun spreading_the_fingers_reports_the_span_ratio_about_the_focal_point() {
        twoDown()
        moveTo(300f, 500f, 700f, 500f)
        val step = steps.single()
        assertEquals(2f, step.factor, 1e-3f)
        assertEquals(500f, step.toX, 1e-3f)
    }

    @Test
    fun steps_chain_so_their_factors_multiply_to_the_total_pinch() {
        twoDown()
        moveTo(300f, 500f, 700f, 500f)
        moveTo(200f, 500f, 800f, 500f)
        assertEquals(3f, steps.fold(1f) { acc, s -> acc * s.factor }, 1e-3f)
    }

    @Test
    fun a_lifted_and_replaced_finger_does_not_jump_the_view() {
        twoDown()
        moveTo(500f, 520f, 700f, 520f)
        nav.up(1, 100)
        nav.down(1, 900f, 100f, 150)
        val before = steps.size
        moveTo(500f, 520f, 910f, 110f)
        assertEquals("a re-landed finger starts a new gesture only past the slop", before, steps.size)
        moveTo(500f, 520f, 960f, 110f)
        val step = steps[before]
        assertEquals(700f, step.fromX, 1e-3f)
        assertEquals(310f, step.fromY, 1e-3f)
    }

    @Test
    fun cancelling_a_navigation_reports_it_as_settled() {
        twoDown()
        moveTo(500f, 520f, 700f, 520f)
        nav.cancel()
        assertEquals(listOf(true, false), active)
    }

    private companion object {
        const val SLOP = 24f
    }
}
