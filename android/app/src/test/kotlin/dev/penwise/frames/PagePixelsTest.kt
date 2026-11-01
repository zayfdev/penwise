package dev.penwise.frames

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** A frame's WebView fits the window in its current orientation; the page is zoomed to keep the preset layout. */
class PagePixelsTest {
    private fun spec(
        width: Int,
        height: Int,
    ) = FrameSpec("f", "p", "/", "desktop", "http://x/", 0f, 0f, width, height, full = true, boxHeight = height)

    @Test
    fun the_desktop_preset_on_a_tab_s9_in_landscape_fills_the_screen_exactly() {
        // Before: 3060 × 1912 device px in a 1600 px window, and Chromium left the bottom 312 px unpainted.
        val pixels = PagePixels.of(spec(1440, 900), density = 2.125f, windowWidth = 2560, windowHeight = 1600)
        assertEquals(2560, pixels.width)
        assertEquals(1600, pixels.height)
        assertEquals(2560f / 1440f, pixels.pxPerCss, 1e-6f)
        assertEquals(2560f / 1440f / 2.125f, pixels.zoom, 1e-6f)
    }

    @Test
    fun the_viewport_meta_lays_the_page_out_at_the_preset_width_under_a_fixed_zoom() {
        val pixels = PagePixels.of(spec(1440, 900), density = 2.125f, windowWidth = 2560, windowHeight = 1600)
        val meta = pixels.viewportMeta(1440)
        assertEquals(
            "width=1440, initial-scale=0.836601, minimum-scale=0.836601, maximum-scale=0.836601, user-scalable=no",
            meta,
        )
    }

    @Test
    fun a_tall_phone_preset_stays_within_the_short_side() {
        val pixels = PagePixels.of(spec(390, 844), density = 2.125f, windowWidth = 2560, windowHeight = 1600)
        assertTrue(pixels.height <= 1600)
        assertEquals(739, pixels.width)
        // The layout height the page sees (view height ÷ px per CSS px) is the preset's, to under a CSS px.
        assertEquals(844f, pixels.height / pixels.pxPerCss, 1f)
    }

    @Test
    fun the_desktop_preset_on_a_tab_s9_in_portrait_is_as_wide_as_the_window() {
        // Before: 2560 × 1600 in a 1600 px wide window, drawn at 0.625; at the end of a long page Chromium capped
        // the visual viewport to the part of the view inside the window (562 of 900 CSS px).
        val pixels = PagePixels.of(spec(1440, 900), density = 2.125f, windowWidth = 1600, windowHeight = 2560)
        assertEquals(1600, pixels.width)
        assertEquals(1000, pixels.height)
        assertEquals(1600f / 1440f, pixels.pxPerCss, 1e-6f)
    }

    @Test
    fun the_locked_camera_never_draws_a_page_smaller_than_it_is_laid_out() {
        val windows = listOf(2560 to 1600, 1600 to 2560, 1200 to 1920)
        for ((width, height) in windows) {
            for ((cssWidth, cssHeight) in listOf(1440 to 900, 834 to 1194, 390 to 844)) {
                val pixels = PagePixels.of(spec(cssWidth, cssHeight), 2.125f, width, height)
                val fitWidth = width.toFloat() / cssWidth
                val label = "$cssWidth x $cssHeight in $width x $height"
                assertTrue(label, fitWidth / pixels.pxPerCss >= 1f - 1e-4f)
                assertTrue(pixels.width <= width && pixels.height <= height)
            }
        }
    }

    @Test
    fun a_preset_that_already_fits_is_not_zoomed() {
        val pixels = PagePixels.of(spec(400, 300), density = 2f, windowWidth = 2560, windowHeight = 1600)
        assertEquals(800, pixels.width)
        assertEquals(600, pixels.height)
        assertEquals(1f, pixels.zoom, 1e-6f)
    }
}
