package dev.penwise.frames

import dev.penwise.ink.ViewTransform

/** A frame's rectangle on the canvas page, in page px. */
data class FrameRect(
    val x: Float,
    val y: Float,
    val width: Float,
    val height: Float,
)

/**
 * Where a frame lands on screen (window px) and how much its page is scaled there. The page inside the frame is laid
 * out at its preset size and only scaled, so its CSS breakpoints see the preset and never the zoom.
 */
data class FramePlacement(
    val left: Float,
    val top: Float,
    val width: Float,
    val height: Float,
    val scale: Float,
) {
    val right: Float get() = left + width

    val bottom: Float get() = top + height

    /** Half-open, like a `View`'s bounds: the right and bottom edges already belong to the next pixel. */
    fun contains(
        x: Float,
        y: Float,
    ): Boolean = x >= left && x < right && y >= top && y < bottom

    /** A window point in the frame's own layout px: the coordinate a `MotionEvent` for its WebView needs. */
    fun toLocalX(windowX: Float): Float = (windowX - left) / scale

    fun toLocalY(windowY: Float): Float = (windowY - top) / scale

    /** The page's own CSS viewport coordinates of a window point (the bridge's hit and rect space). */
    fun toCssX(
        windowX: Float,
        pxPerCss: Float,
    ): Float = (windowX - left) / (scale * pxPerCss)

    fun toCssY(
        windowY: Float,
        pxPerCss: Float,
    ): Float = (windowY - top) / (scale * pxPerCss)
}

/**
 * Page rect + canvas view → screen placement, and back; the canvas pan/zoom transforms the frames with it.
 *
 * A frame's item box is its page box in page units (`css_w × css_h` at scale 1, C1), while the WebView inside it is
 * laid out at `css_w × pxPerCss` device px ([PagePixels]) with the page zoomed so its breakpoints see the preset.
 * [pxPerCss] turns the canvas scale into the scale of the view's own pixels.
 */
object FrameGeometry {
    fun place(
        rect: FrameRect,
        view: ViewTransform,
        pxPerCss: Float,
    ): FramePlacement =
        FramePlacement(
            left = view.toScreenX(rect.x),
            top = view.toScreenY(rect.y),
            width = rect.width * view.scale,
            height = rect.height * view.scale,
            scale = view.scale / pxPerCss,
        )

    fun placeAll(
        rects: List<FrameRect>,
        view: ViewTransform,
        pxPerCss: Float,
    ): List<FramePlacement> = rects.map { place(it, view, pxPerCss) }

    /** Index of the first placement containing the window point, or -1. */
    fun at(
        x: Float,
        y: Float,
        placements: List<FramePlacement>,
    ): Int = placements.indexOfFirst { it.contains(x, y) }
}
