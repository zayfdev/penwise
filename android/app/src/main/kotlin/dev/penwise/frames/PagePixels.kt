package dev.penwise.frames

import java.util.Locale
import kotlin.math.floor
import kotlin.math.min

/**
 * How a frame's page maps onto its WebView: the view's size in device px and how many of them one CSS px takes.
 *
 * Chromium sizes a WebView's visual viewport from the part of the view that is visible in the window, measured in
 * window px without the view's own scale. A view laid out larger than it is drawn (the locked camera draws it scaled
 * by `fit-width scale ÷ pxPerCss`) is therefore cut short wherever the window clips it: on the Tab S9 the 1440 × 900
 * preset used to be 2560 × 1600 in portrait too and drawn at 0.625, and once the camera hit the end of a long page
 * (the window slid down the screen) `visualViewport.height` fell to 562 of 900 and the page stopped short. So the
 * page is zoomed until its width fits the window's width and its height the window's height, and never past the
 * display density: the locked camera's fit-width scale is then never below `pxPerCss` and the view is never drawn
 * smaller than it is laid out (desktop: 2560 × 1600 in landscape, 1600 × 1000 in portrait). The phone and tablet
 * presets are still taller than the screen at fit-width in landscape (the phone also in portrait), so a locked camera
 * shows only the top of their window and the view hangs off the screen's end; that was so before and is not changed
 * here. The bridge sets the viewport `<meta>` from [viewportMeta]; the page still lays out at the preset's CSS size.
 */
internal data class PagePixels(
    val width: Int,
    val height: Int,
    /** Device px per CSS px inside the page: the display density times the page zoom. */
    val pxPerCss: Float,
    /** The page zoom in device-independent px per CSS px (1 = no zoom). */
    val zoom: Float,
) {
    /** The viewport `<meta>` content that lays the page out at [cssWidth] CSS px under this zoom, fixed. */
    fun viewportMeta(cssWidth: Int): String {
        val scale = String.format(Locale.ROOT, "%.6f", zoom)
        return "width=$cssWidth, initial-scale=$scale, minimum-scale=$scale, maximum-scale=$scale, user-scalable=no"
    }

    companion object {
        private const val EPSILON = 1e-6

        /** The pixels of [spec] in a window of [window] device px (the screen in its current orientation). */
        fun on(
            spec: FrameSpec,
            density: Float,
            window: WindowSize,
        ): PagePixels = of(spec, density, window.width, window.height)

        /** The pixels of [spec] on a display of [density] whose window is [windowWidth] × [windowHeight] device px. */
        fun of(
            spec: FrameSpec,
            density: Float,
            windowWidth: Int,
            windowHeight: Int,
        ): PagePixels {
            val cssWidth = spec.cssWidth.coerceAtLeast(1)
            val cssHeight = spec.cssHeight.coerceAtLeast(1)
            var pxPerCss = density.toDouble()
            if (windowWidth > 0) pxPerCss = min(pxPerCss, windowWidth.toDouble() / cssWidth)
            if (windowHeight > 0) pxPerCss = min(pxPerCss, windowHeight.toDouble() / cssHeight)
            // The width is rounded down first and sets the zoom, so the page's layout width is exactly the preset;
            // the epsilon keeps an exact fit (1440 × 2560 / 1440 = 2560) from rounding down a pixel.
            val width = floor(cssWidth * pxPerCss + EPSILON).toInt().coerceAtLeast(1)
            val height = floor(cssHeight.toDouble() * width / cssWidth + EPSILON).toInt().coerceAtLeast(1)
            val exact = width.toFloat() / cssWidth
            return PagePixels(width, height, exact, exact / density)
        }
    }
}

/** The size in device px of the window the frames are drawn in: the screen in its current orientation. */
internal data class WindowSize(
    val width: Int,
    val height: Int,
)
