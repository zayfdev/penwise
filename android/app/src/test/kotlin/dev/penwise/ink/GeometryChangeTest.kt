package dev.penwise.ink

import dev.penwise.core.InkGeometry
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** The front buffer repaints only the box around what changed between two frames; everything that did must be in it. */
class GeometryChangeTest {
    private fun outline(vararg xy: Float) = InkGeometry(false, xy, intArrayOf(xy.size / 2), FloatArray(0), 0, 1f, false)

    /** A strip along y = 0: top side left to right, then bottom side right to left; [tip] is where the pen is. */
    private fun strip(
        tip: Float,
        length: Int = 100,
    ): InkGeometry {
        val top = ArrayList<Float>()
        val bottom = ArrayList<Float>()
        for (x in 0..length) {
            top += listOf(x.toFloat(), -5f)
            bottom += listOf(x.toFloat(), 5f)
        }
        val xy = top + listOf(tip + 3f, 0f) + bottom.chunked(2).reversed().flatten()
        return outline(*xy.toFloatArray())
    }

    @Test
    fun identical_frames_change_nothing() {
        assertFalse(GeometryChange.between(strip(100f), strip(100f), FloatArray(4)))
    }

    @Test
    fun a_grown_tail_changes_only_the_box_around_the_tail() {
        val box = FloatArray(4)
        val old = strip(100f, 90)
        val new = strip(100f, 100)
        assertTrue(GeometryChange.between(old, new, box))
        assertTrue("left edge ${box[0]} must be near the old tip, not the start of the stroke", box[0] > 80f)
        assertTrue(box[2] >= 100f)
        assertTrue(box[1] <= -5f && box[3] >= 5f)
    }

    @Test
    fun every_vertex_that_moved_lies_inside_the_box() {
        val old = strip(70f, 70)
        val new = strip(80f, 80)
        val box = FloatArray(4)
        assertTrue(GeometryChange.between(old, new, box))
        for ((a, b) in listOf(old to new, new to old)) {
            for (i in 0 until a.outline.size / 2) {
                val x = a.outline[2 * i]
                val y = a.outline[2 * i + 1]
                val kept = (0 until b.outline.size / 2).any { b.outline[2 * it] == x && b.outline[2 * it + 1] == y }
                if (!kept) assertTrue("($x, $y) moved", x in box[0]..box[2] && y in box[1]..box[3])
            }
        }
    }

    @Test
    fun stamps_change_from_the_first_new_or_moved_dab() {
        fun dabs(vararg x: Float): InkGeometry {
            val all = FloatArray(x.size * 9) { if (it % 9 == 0) x[it / 9] else 2f }
            return InkGeometry(true, FloatArray(0), IntArray(0), all, 0, 1f, false)
        }
        val box = FloatArray(4)
        assertTrue(GeometryChange.between(dabs(0f, 10f, 20f), dabs(0f, 10f, 25f, 30f), box))
        assertEquals(16f, box[0], 0f)
        assertEquals(34f, box[2], 0f)
    }

    @Test
    fun a_different_kind_of_geometry_changes_everything() {
        val box = FloatArray(4)
        val dab = FloatArray(9) { if (it == 2 || it == 3) 4f else 50f }
        val stamps = InkGeometry(true, FloatArray(0), IntArray(0), dab, 0, 1f, false)
        assertTrue(GeometryChange.between(strip(10f, 10), stamps, box))
        assertTrue(box[0] < 0f && box[2] > 50f)
    }
}
