package dev.penwise.tools

import androidx.compose.runtime.mutableStateOf
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** One flyout at a time, the page panel being the screen's drawer. */
class DockFlyoutsTest {
    private class Rig {
        val others = mutableStateOf<DockFlyout?>(null)
        var moreOpen = false
        val flyouts get() = DockFlyouts(others, moreOpen) { moreOpen = it }
    }

    @Test
    fun a_button_opens_its_flyout_and_the_same_button_closes_it() {
        val rig = Rig()
        rig.flyouts.toggle(DockFlyout.PEN)
        assertEquals(DockFlyout.PEN, rig.flyouts.open)
        rig.flyouts.toggle(DockFlyout.PEN)
        assertNull(rig.flyouts.open)
    }

    @Test
    fun opening_another_replaces_the_open_one() {
        val rig = Rig()
        rig.flyouts.toggle(DockFlyout.PEN)
        rig.flyouts.toggle(DockFlyout.COLOUR)
        assertEquals(DockFlyout.COLOUR, rig.flyouts.open)
        assertTrue(rig.flyouts.isOpen(DockFlyout.COLOUR))
        assertFalse(rig.flyouts.isOpen(DockFlyout.PEN))
    }

    @Test
    fun the_page_panel_is_the_screens_drawer() {
        val rig = Rig()
        rig.flyouts.toggle(DockFlyout.SHAPES)
        rig.flyouts.toggle(DockFlyout.MORE)
        assertTrue(rig.moreOpen)
        assertEquals(DockFlyout.MORE, rig.flyouts.open)
        rig.flyouts.toggle(DockFlyout.PEN)
        assertFalse(rig.moreOpen)
        assertEquals(DockFlyout.PEN, rig.flyouts.open)
    }

    @Test
    fun the_pen_touching_the_page_closes_everything() {
        val rig = Rig()
        rig.flyouts.toggle(DockFlyout.MORE)
        rig.flyouts.close()
        assertNull(rig.flyouts.open)
        assertFalse(rig.moreOpen)
        rig.flyouts.toggle(DockFlyout.COLOUR)
        rig.flyouts.close()
        assertNull(rig.flyouts.open)
    }

    @Test
    fun back_closes_an_open_flyout_before_leaving_the_page() {
        val rig = Rig()
        rig.flyouts.toggle(DockFlyout.COLOUR)
        assertTrue(rig.flyouts.handleBack())
        assertNull(rig.flyouts.open)
        assertFalse(rig.flyouts.handleBack())
    }

    @Test
    fun back_closes_the_page_panel_too() {
        val rig = Rig()
        rig.flyouts.toggle(DockFlyout.MORE)
        assertTrue(rig.flyouts.handleBack())
        assertFalse(rig.moreOpen)
    }

    @Test
    fun the_eraser_flyout_is_one_of_the_flyouts() {
        val rig = Rig()
        rig.flyouts.toggle(DockFlyout.PEN)
        rig.flyouts.toggle(DockFlyout.ERASER)
        assertEquals(DockFlyout.ERASER, rig.flyouts.open)
    }
}
