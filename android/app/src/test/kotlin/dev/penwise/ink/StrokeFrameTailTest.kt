package dev.penwise.ink

import dev.penwise.core.InkGeometry
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** A predicted tail is a second layer over the stroke tip: only a solid normal-blend outline stroke may have one. */
class StrokeFrameTailTest {
    private fun outline(
        opacity: Float,
        multiply: Boolean,
    ) = InkGeometry(false, FloatArray(0), IntArray(0), FloatArray(0), 0xFF000000.toInt(), opacity, multiply)

    @Test
    fun a_solid_outline_stroke_has_a_tail() {
        assertTrue(StrokeFrame.hasTail(outline(1f, false)))
    }

    @Test
    fun a_translucent_stroke_has_none() {
        assertFalse(StrokeFrame.hasTail(outline(0.35f, false)))
    }

    @Test
    fun a_multiply_stroke_has_none() {
        assertFalse(StrokeFrame.hasTail(outline(1f, true)))
    }

    @Test
    fun a_stamp_stroke_has_none() {
        val stamps = InkGeometry(true, FloatArray(0), IntArray(0), FloatArray(0), 0xFF000000.toInt(), 1f, false)
        assertFalse(StrokeFrame.hasTail(stamps))
    }
}
