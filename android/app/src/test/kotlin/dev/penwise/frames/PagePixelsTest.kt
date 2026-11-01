package dev.penwise.frames

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** A frame's WebView is never taller than the screen's shorter side; the page is zoomed to keep the preset layout. */
class PagePixelsTest {
    private fun spec(
        width: Int,
        height: Int,
    ) = FrameSpec("f", "p", "/", "desktop", "http://x/", 0f, 0f, width, height, full = true, boxHeight = height)

    @Test
    fun the_desktop_preset_on_a_tab_s9_in_landscape_fills_the_screen_exactly() {
        // Before: 3060 × 1912 device px in a 1600 px window, and Chromium left the bottom 312 px unpainted.
        val pixels = PagePixels.of(spec(1440, 900), density = 2.125f, shortSide = 1600)
        assertEquals(2560, pixels.width)
        assertEquals(1600, pixels.height)
        assertEquals(2560f / 1440f, pixels.pxPerCss, 1e-6f)
        assertEquals(2560f / 1440f / 2.125f, pixels.zoom, 1e-6f)
    }

    @Test
    fun the_viewport_meta_lays_the_page_out_at_the_preset_width_under_a_fixed_zoom() {
        val meta = PagePixels.of(spec(1440, 900), density = 2.125f, shortSide = 1600).viewportMeta(1440)
        assertEquals(
            "width=1440, initial-scale=0.836601, minimum-scale=0.836601, maximum-scale=0.836601, user-scalable=no",
            meta,
        )
    }

    @Test
    fun a_tall_phone_preset_stays_within_the_short_side() {
        val pixels = PagePixels.of(spec(390, 844), density = 2.125f, shortSide = 1600)
        assertTrue(pixels.height <= 1600)
        assertEquals(739, pixels.width)
        // The layout height the page sees (view height ÷ px per CSS px) is the preset's, to under a CSS px.
        assertEquals(844f, pixels.height / pixels.pxPerCss, 1f)
    }

    @Test
    fun a_preset_that_already_fits_is_not_zoomed() {
        val pixels = PagePixels.of(spec(400, 300), density = 2f, shortSide = 1600)
        assertEquals(800, pixels.width)
        assertEquals(600, pixels.height)
        assertEquals(1f, pixels.zoom, 1e-6f)
    }
}
