package dev.penwise.frames

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
}
