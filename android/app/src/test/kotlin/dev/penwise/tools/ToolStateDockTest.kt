package dev.penwise.tools

import dev.penwise.ink.ShapeMode
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** What the dock's tools and the colour flyout write into the tool state. */
class ToolStateDockTest {
    @Test
    fun drawing_leaves_the_eraser_the_lasso_and_the_shapes() {
        val state = ToolState()
        state.toggleShape(ShapeMode.Arrow)
        assertFalse(state.drawing)
        state.draw()
        assertTrue(state.drawing)
        state.toggleEraser()
        assertFalse(state.drawing)
        state.draw()
        state.toggleLasso()
        assertFalse(state.drawing)
        state.draw()
        assertTrue(state.drawing)
        assertFalse(state.lassoMode)
    }

    @Test
    fun a_wheel_colour_is_kept_as_plain_rgb_and_remembered_when_the_drag_ends() {
        val state = ToolState()
        state.selectSwatch(0xFF123456.toInt())
        assertEquals(0x123456, state.swatchRgb)
        assertFalse(state.recents.items.contains(0x123456))
        state.commitColour()
        assertEquals(0x123456, state.recents.items.first())
    }

    @Test
    fun a_pinned_colour_is_not_a_recent_one_and_auto_clears_the_swatch() {
        val state = ToolState()
        val before = state.recents.items
        state.selectSwatch(Palette.RED)
        state.commitColour()
        assertEquals(before, state.recents.items)
        state.selectSwatch(null)
        assertNull(state.swatchRgb)
    }
}
