package dev.penwise.frames

import androidx.compose.ui.unit.IntRect
import androidx.compose.ui.unit.IntSize
import org.junit.Assert.assertEquals
import org.junit.Test

/** Where the chrome windows go: the corners, clear of the cutout, and the chip that follows its frame. */
class ChromePlacementTest {
    private val window = IntSize(2000, 1200)
    private val popup = IntSize(300, 80)
    private val insets = ChromeInsets(left = 0, top = 40, right = 0, bottom = 0)

    private fun at(corner: ChromeCorner) = ChromePlacement.corner(corner, window, popup, 12, insets)

    @Test
    fun the_top_corners_sit_below_the_cutout() {
        assertEquals(ChromeSpot(12, 52), at(ChromeCorner.TOP_START))
        assertEquals(ChromeSpot(2000 - 300 - 12, 52), at(ChromeCorner.TOP_END))
        assertEquals(ChromeSpot(850, 52), at(ChromeCorner.TOP_CENTER))
    }

    @Test
    fun the_bottom_windows_sit_above_the_bottom_edge() {
        assertEquals(ChromeSpot(12, 1200 - 80 - 12), at(ChromeCorner.BOTTOM_START))
        assertEquals(ChromeSpot(850, 1200 - 80 - 12), at(ChromeCorner.BOTTOM_CENTER))
    }

    @Test
    fun a_following_window_keeps_its_point_inside_the_screen() {
        assertEquals(ChromeSpot(500, 300), ChromePlacement.pinned(500, 300, window, popup, 12, insets))
        assertEquals(ChromeSpot(12, 52), ChromePlacement.pinned(-400, -900, window, popup, 12, insets))
        assertEquals(
            ChromeSpot(2000 - 300 - 12, 1200 - 80 - 12),
            ChromePlacement.pinned(5000, 5000, window, popup, 12, insets),
        )
    }

    @Test
    fun a_window_bigger_than_the_screen_sticks_to_the_start() {
        val huge = IntSize(3000, 100)
        assertEquals(ChromeSpot(12, 300), ChromePlacement.pinned(700, 300, window, huge, 12, insets))
    }

    private val chip = IntSize(1000, 90)

    @Test
    fun in_portrait_the_chip_drops_below_the_toolbar_it_would_cover() {
        val screen = IntSize(1600, 2560)
        val toolbar = IntRect(500, 52, 1100, 160)
        val pinned = ChromePlacement.pinned(20, 60, screen, chip, 12, insets)
        val spot = ChromePlacement.clearOf(pinned, chip, toolbar, 12, screen, insets)
        assertEquals(ChromeSpot(pinned.x, 172), spot)
    }

    @Test
    fun in_landscape_the_chip_stays_beside_the_toolbar_when_it_fits() {
        val screen = IntSize(2560, 1600)
        val toolbar = IntRect(980, 52, 1580, 160)
        val narrow = IntSize(800, 90)
        val pinned = ChromePlacement.pinned(20, 60, screen, narrow, 12, insets)
        assertEquals(pinned, ChromePlacement.clearOf(pinned, narrow, toolbar, 12, screen, insets))
        val wide = ChromePlacement.pinned(20, 60, screen, chip, 12, insets)
        val moved = ChromePlacement.clearOf(wide, chip, toolbar, 12, screen, insets)
        assertEquals(172, moved.y)
    }

    @Test
    fun a_chip_far_below_the_toolbar_is_left_alone() {
        val screen = IntSize(1600, 2560)
        val spot = ChromeSpot(20, 900)
        assertEquals(spot, ChromePlacement.clearOf(spot, chip, IntRect(500, 52, 1100, 160), 12, screen, insets))
    }
}
