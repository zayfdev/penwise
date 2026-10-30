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
    ): FrameRect =
        FrameRect(
            frame.x + relX + if (moves) cssX - scrollX else 0f,
            frame.y + relY + if (moves) cssY - scrollY else 0f,
            frame.cssWidth.toFloat(),
            frame.cssHeight.toFloat(),
        )

    companion object {
        /** Before the core has said anything: the window sits at the frame's own corner. */
        fun at(moves: Boolean) = WindowBase(0f, 0f, 0f, 0f, moves)
    }
}

/** The maths of the scroll link, apart from any view so it is tested without a WebView. */
object ScrollLink {
    /** A WebView's scroll offset in device px as CSS px (a WebView at 100 % page zoom: one CSS px is [density] px). */
    fun cssOffset(
        devicePx: Int,
        density: Float,
        pageZoom: Float = 1f,
    ): Float = devicePx / (density * pageZoom)

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
