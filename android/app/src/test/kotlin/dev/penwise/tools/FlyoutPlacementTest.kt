package dev.penwise.tools

import androidx.compose.ui.unit.IntRect
import androidx.compose.ui.unit.IntSize
import org.junit.Assert.assertEquals
import org.junit.Test

class FlyoutPlacementTest {
    private val window = IntSize(2000, 1200)
    private val flyout = IntSize(400, 500)

    @Test
    fun a_flyout_is_centred_above_its_button() {
        val spot = FlyoutPlacement.above(IntRect(900, 1100, 948, 1148), window, flyout, gap = 20, margin = 12)
        assertEquals(FlyoutSpot(x = 724, y = 580, pivotX = 0.5f), spot)
    }

    @Test
    fun near_an_edge_it_slides_back_and_grows_from_the_button() {
        val spot = FlyoutPlacement.above(IntRect(20, 1100, 68, 1148), window, flyout, gap = 20, margin = 12)
        assertEquals(12, spot.x)
        assertEquals(0.08f, spot.pivotX, 0.001f)
        val right = FlyoutPlacement.above(IntRect(1950, 1100, 1998, 1148), window, flyout, gap = 20, margin = 12)
        assertEquals(2000 - 12 - 400, right.x)
        assertEquals(0.965f, right.pivotX, 0.001f)
    }

    @Test
    fun a_flyout_taller_than_the_room_stops_at_the_top_margin() {
        val spot = FlyoutPlacement.above(IntRect(900, 300, 948, 348), window, flyout, gap = 20, margin = 12)
        assertEquals(12, spot.y)
    }
}
