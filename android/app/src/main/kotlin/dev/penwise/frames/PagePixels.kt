package dev.penwise.frames

import android.util.DisplayMetrics
import java.util.Locale
import kotlin.math.floor
import kotlin.math.min

/**
 * How a frame's page maps onto its WebView: the view's size in device px and how many of them one CSS px takes.
 *
 * Chromium caps a page's visual viewport at the window's height when the WebView is taller than its window, and
 * leaves the rest of the view unpainted after a touch scroll (Tab S9 landscape: the 1440 × 900 preset at density
 * 2.125 is 1912 px tall in a 1600 px window, so the bottom 16 % of a fitted frame went blank). So the view is never
 * taller than the screen's shorter side, and the page is zoomed by the same factor: the bridge sets the viewport
 * `<meta>` from [viewportMeta], and the page still lays out at the preset's CSS size.
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

        /** The pixels of [spec] on this display: no taller than its shorter side. */
        fun on(
            spec: FrameSpec,
            metrics: DisplayMetrics,
        ): PagePixels = of(spec, metrics.density, min(metrics.widthPixels, metrics.heightPixels))

        /** The pixels of [spec] on a display of [density] whose shorter side is [shortSide] device px. */
        fun of(
            spec: FrameSpec,
            density: Float,
            shortSide: Int,
        ): PagePixels {
            val cssWidth = spec.cssWidth.coerceAtLeast(1)
            val tall = spec.cssHeight.toDouble() * density
            val fit = if (tall > 0.0 && shortSide > 0) min(1.0, shortSide / tall) else 1.0
            // The width is rounded down first and sets the zoom, so the page's layout width is exactly the preset;
            // the epsilon keeps an exact fit (3060 × 1600 / 1912.5 = 2560) from rounding down a pixel.
            val width = floor(cssWidth * density * fit + EPSILON).toInt().coerceAtLeast(1)
            val height = floor(spec.cssHeight.toDouble() * width / cssWidth + EPSILON).toInt().coerceAtLeast(1)
            val pxPerCss = width.toFloat() / cssWidth
            return PagePixels(width, height, pxPerCss, pxPerCss / density)
        }
    }
}
