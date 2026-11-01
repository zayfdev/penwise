package dev.penwise.frames

import kotlin.math.max

/**
 * The live window of a full-length frame follows the page's own scroll (contract §5). A base is what the core said
 * (`frame_live_window`) for the scroll the bridge had reported then, kept relative to the frame's position so a frame
 * that moves (the owner drags it, a resolved round pushes it right) takes its window along. The WebView's own scroll,
 * read in the same UI frame, moves the window from there. The core owns the position rule; a viewport frame's window
 * does not move with the scroll ([moves] false), a full frame's moves one-to-one.
 */
class WindowBase(
    /** The window's corner minus the frame's position, as the core computed it. */
    val relX: Float,
    val relY: Float,
    val scrollX: Float,
    val scrollY: Float,
    val moves: Boolean,
) {
    /** The window on the board for [frame] at its position now and the page scrolled to ([cssX], [cssY]) CSS px. */
    fun rect(
        frame: FrameSpec,
        cssX: Float,
        cssY: Float,
        documentHeight: Float = 0f,
    ): FrameRect =
        FrameRect(
            frame.x + relX + if (moves) cssX - scrollX else 0f,
            frame.y + relY + if (moves) windowY(cssY, documentHeight, frame) - scrollY else 0f,
            frame.cssWidth.toFloat(),
            frame.cssHeight.toFloat(),
        )

    private fun windowY(
        cssY: Float,
        documentHeight: Float,
        frame: FrameSpec,
    ) = ScrollLink.windowScroll(cssY, documentHeight, frame.cssHeight.toFloat())

    companion object {
        /** Before the core has said anything: the window sits at the frame's own corner. */
        fun at(moves: Boolean) = WindowBase(0f, 0f, 0f, 0f, moves)
    }
}

/** The maths of the scroll link, apart from any view so it is tested without a WebView. */
object ScrollLink {
    /** A WebView's scroll offset in device px as CSS px; [pxPerCss] is the page's [PagePixels.pxPerCss]. */
    fun cssOffset(
        devicePx: Int,
        pxPerCss: Float,
    ): Float = devicePx / pxPerCss

    /**
     * The page scroll the live window is drawn at: [cssY] held to the document's live end. The WebView's own offset
     * rounds up to whole device px and can sit a px or two past the page's real maximum; a window drawn there hangs
     * off the screen's end, Chromium then shrinks the page's visual viewport and the offset runs further (a loop).
     * An end that is not known ([documentHeight] no taller than the window) clamps nothing.
     */
    fun windowScroll(
        cssY: Float,
        documentHeight: Float,
        windowHeight: Float,
    ): Float = if (documentHeight <= windowHeight) cssY else cssY.coerceAtMost(documentHeight - windowHeight)

    /**
     * The document's live height in CSS px: the tallest of the frame's box (it reaches the app late), the bridge's last
     * report (it comes after the scroll event) and the WebView's own content height (always current).
     */
    fun liveDocument(
        boxHeight: Float,
        bridgeHeight: Float,
        contentHeight: Float,
    ): Float = max(boxHeight, max(bridgeHeight, contentHeight))

    /**
     * The camera translation (screen px) along one axis that keeps the content under the finger when the page scrolled
     * by [deltaCss] while the window slid with it: the camera moves the opposite way, scaled to the screen. Zero for a
     * frame whose window does not move.
     */
    fun cameraPan(
        deltaCss: Float,
        scale: Float,
        moves: Boolean,
    ): Float = if (moves) -deltaCss * scale else 0f

    /**
     * The scroll (CSS px) that makes the window cover the visible part of the frame, [visibleTop]..[visibleBottom] in
     * document px: centred on it when the window is taller, top-aligned when it is not, clamped to the document.
     */
    fun coverScroll(
        visibleTop: Float,
        visibleBottom: Float,
        windowHeight: Float,
        documentHeight: Float,
    ): Float {
        val visible = visibleBottom - visibleTop
        val wanted = if (visible >= windowHeight) visibleTop else visibleTop - (windowHeight - visible) / 2f
        return wanted.coerceIn(0f, max(0f, documentHeight - windowHeight))
    }

    /** Does a window at [windowTop] (document px) of height [windowHeight] already show all of the visible part? */
    fun covers(
        windowTop: Float,
        windowHeight: Float,
        visibleTop: Float,
        visibleBottom: Float,
    ): Boolean = windowTop <= visibleTop + COVER_SLACK && windowTop + windowHeight >= visibleBottom - COVER_SLACK

    private const val COVER_SLACK = 0.5f
}
