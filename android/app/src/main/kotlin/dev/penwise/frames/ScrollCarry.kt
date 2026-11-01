package dev.penwise.frames

import kotlin.math.roundToInt

/** How far a WebView can scroll (view px) and where it is now. */
internal class ScrollBounds(
    val x: Int,
    val y: Int,
    val maxX: Int,
    val maxY: Int,
)

/**
 * Screen px of a finger drag as the whole view px a WebView scrolls by. A WebView view sits scaled by `scale` (screen
 * px per view px) under the camera, so a drag of d screen px is d / scale view px, which is d / camera scale CSS px.
 * The sub-pixel rest is kept, so a slow drag loses nothing. The result never takes the page past its scroll range, so
 * the WebView never reports an overshoot; the rest is dropped at an end, so the drag back is not delayed.
 */
internal class ScrollCarry {
    private var restX = 0f
    private var restY = 0f

    /** Whole view px for the screen-px drag ([dx], [dy]); the content follows the finger, so the sign flips. */
    fun viewPx(
        dx: Float,
        dy: Float,
        scale: Float,
        bounds: ScrollBounds? = null,
    ): Pair<Int, Int> {
        val wantX = restX - dx / scale
        val wantY = restY - dy / scale
        var x = wantX.roundToInt()
        var y = wantY.roundToInt()
        restX = wantX - x
        restY = wantY - y
        if (bounds != null) {
            val cx = x.coerceIn(-bounds.x, bounds.maxX - bounds.x)
            val cy = y.coerceIn(-bounds.y, bounds.maxY - bounds.y)
            if (cx != x) restX = 0f
            if (cy != y) restY = 0f
            x = cx
            y = cy
        }
        return x to y
    }

    /** The fling velocity for the WebView (view px/s): the content follows the finger, so the sign flips. */
    fun flingView(
        vx: Float,
        vy: Float,
        scale: Float,
    ): Pair<Int, Int> = (-vx / scale).roundToInt() to (-vy / scale).roundToInt()

    fun reset() {
        restX = 0f
        restY = 0f
    }
}
