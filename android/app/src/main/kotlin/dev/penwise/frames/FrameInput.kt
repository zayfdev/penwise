package dev.penwise.frames

/** Who owns the pointer sequence a touch begins. */
enum class FrameTarget {
    /** The stylus: it always draws, over a page or not. */
    INK,

    /** One finger on a page: the WebView scrolls, clicks, hovers. */
    FRAME,

    /** Two fingers anywhere, or a second finger joining: the canvas pans and zooms as always. */
    CANVAS,

    /** One finger outside every page: nothing (it is usually the palm). */
    NONE,
}

/**
 * The touch routing rules of the harness (§6.4, §12.7), free of Android types so they can be tested: the stylus always
 * draws, two fingers always pan/zoom the canvas, one finger belongs to the page under it, and one finger outside every
 * frame belongs to nobody.
 */
object FrameTouch {
    fun target(
        hasStylus: Boolean,
        fingers: Int,
        x: Float,
        y: Float,
        placements: List<FramePlacement>,
    ): FrameTarget =
        when {
            hasStylus -> FrameTarget.INK
            fingers >= 2 -> FrameTarget.CANVAS
            fingers == 1 && FrameGeometry.at(x, y, placements) >= 0 -> FrameTarget.FRAME
            else -> FrameTarget.NONE
        }

    /** A gesture already given to a page keeps it while one finger is down; a second finger hands it to the canvas. */
    fun keepsFrame(fingers: Int): Boolean = fingers <= 1
}
