package dev.penwise.ink

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class ItemRenderTest {
    /** An outline part: colour, opacity, one triangle contour. */
    private fun outline(kind: Float): FloatArray {
        val payload = floatArrayOf(10f, 20f, 30f, 255f, 1f, 1f, 3f, 0f, 0f, 10f, 0f, 0f, 10f)
        return floatArrayOf(kind, payload.size.toFloat()) + payload
    }

    @Test
    fun `a shape's fill part (kind 6) is drawn with the Normal blend, its outline keeps multiply`() {
        val floats = floatArrayOf(1f, 2f) + outline(6f) + outline(0f)
        val item = requireNotNull(ItemRender.parse(floats) { "" })
        assertTrue(item.multiply)
        val (fill, stroke) = item.parts.map { it as RenderPart.Outline }
        assertTrue("the fill ignores the item's multiply", fill.normalBlend)
        assertFalse("the outline follows it", stroke.normalBlend)
        assertEquals(3, fill.points.size / 2)
    }
}
