package dev.penwise.ink

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** Lasso and shape-guide frames clear only what changed, not their whole box, each event. */
class FrontChangeTest {
    @Test
    fun a_growing_lasso_changes_only_its_newest_edge() {
        val old = floatArrayOf(0f, 0f, 500f, 0f, 500f, 500f)
        val new = floatArrayOf(0f, 0f, 500f, 0f, 500f, 500f, 520f, 540f)
        val box = FloatArray(4)
        assertTrue(GeometryChange.polyline(old, new, box))
        assertTrue("the old far corner is left alone", box[GeometryChange.LEFT] >= 400f)
        assertTrue(box[GeometryChange.TOP] >= 0f && box[GeometryChange.TOP] < 500f)
        assertTrue(box[GeometryChange.RIGHT] >= 520f && box[GeometryChange.BOTTOM] >= 540f)
    }

    @Test
    fun an_unchanged_lasso_changes_nothing() {
        val points = floatArrayOf(0f, 0f, 10f, 10f)
        assertFalse(GeometryChange.polyline(points, points.copyOf(), FloatArray(4)))
    }

    @Test
    fun dragging_a_rectangle_sideways_clears_the_right_edge_strip_only() {
        val out = DirtyRects()
        RectGuideChange.between(100f, 100f, 400f, 300f, 420f, 300f, 4f, out)
        assertEquals(1, out.size)
        assertEquals(396f, out.left(0), 0f)
        assertEquals(424f, out.right(0), 0f)
        assertEquals(96f, out.top(0), 0f)
        assertEquals(304f, out.bottom(0), 0f)
    }

    @Test
    fun dragging_a_rectangle_diagonally_clears_two_edge_strips_not_the_box() {
        val out = DirtyRects()
        RectGuideChange.between(0f, 0f, 1000f, 800f, 1010f, 810f, 4f, out)
        assertEquals(2, out.size)
        for (i in 0 until out.size) {
            val area = (out.right(i) - out.left(i)) * (out.bottom(i) - out.top(i))
            assertTrue("strip $i is thin: $area", area < 30_000f)
        }
    }

    @Test
    fun a_still_rectangle_clears_nothing() {
        val out = DirtyRects()
        RectGuideChange.between(0f, 0f, 10f, 10f, 10f, 10f, 4f, out)
        assertEquals(0, out.size)
    }
}
