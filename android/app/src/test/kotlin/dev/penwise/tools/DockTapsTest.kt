package dev.penwise.tools

import org.junit.Assert.assertEquals
import org.junit.Test

/** What a tap on a tool button does: switch tools, or open and close the tool's flyout. */
class DockTapsTest {
    @Test
    fun the_pen_button_activates_the_pen_first_and_then_opens_its_flyout() {
        assertEquals(ToolTap.ACTIVATE, DockTaps.pen(inUse = false, flyoutOpen = false))
        assertEquals(ToolTap.TOGGLE_FLYOUT, DockTaps.pen(inUse = true, flyoutOpen = false))
    }

    @Test
    fun with_the_pen_flyout_open_the_pen_button_only_closes_it_even_after_picking_the_marker() {
        // Marker picked inside the flyout: the pen is no longer "in use", but the tap must not switch tools.
        assertEquals(ToolTap.TOGGLE_FLYOUT, DockTaps.pen(inUse = false, flyoutOpen = true))
    }

    @Test
    fun the_marker_button_activates_then_opens_the_pen_flyout() {
        assertEquals(ToolTap.ACTIVATE, DockTaps.marker(inUse = false))
        assertEquals(ToolTap.TOGGLE_FLYOUT, DockTaps.marker(inUse = true))
    }

    @Test
    fun the_eraser_turns_on_first_and_a_second_tap_opens_its_flyout() {
        assertEquals(ToolTap.ACTIVATE, DockTaps.eraser(eraserOn = false))
        assertEquals(ToolTap.TOGGLE_FLYOUT, DockTaps.eraser(eraserOn = true))
    }
}
