package dev.penwise.frames

import kotlin.math.abs

/**
 * Classifies the edge swipe (locked-frames contract §3.1): one finger that goes down in an edge strip ([EdgeStrips]:
 * within [edgePx] of the left or right screen edge, inside the [stripPx] tall centred band the app excludes from the
 * system gestures) and travels [travelPx] inwards, mostly horizontally, goes to the previous (from the left edge) or
 * next (from the right edge) frame of the switcher. Pure logic over explicit coordinates.
 */
class EdgeSwipe(
    private val edgePx: Float,
    private val travelPx: Float,
    private val stripPx: Float,
) {
    private enum class Side { LEFT, RIGHT }

    private var side: Side? = null
    private var startX = 0f
    private var startY = 0f
    private var done = false
    private var lastX = 0f
    private var lastY = 0f

    /** True while a touch that started in a strip is being followed. */
    val active: Boolean get() = side != null && !done

    /** The touch in a strip so far travelled more sideways than up or down: it may still become an edge swipe. */
    val leansHorizontal: Boolean get() = active && abs(lastX - startX) > abs(lastY - startY)

    /** A finger went down at ([x], [y]) on a [viewWidth] by [viewHeight] screen; true in an edge strip. */
    fun begin(
        x: Float,
        y: Float,
        viewWidth: Float,
        viewHeight: Float,
    ): Boolean {
        reset()
        if (!EdgeStrips.contains(x, y, viewWidth, viewHeight, edgePx, stripPx)) return false
        side = if (x <= edgePx) Side.LEFT else Side.RIGHT
        startX = x
        startY = y
        lastX = x
        lastY = y
        return true
    }

    /**
     * The finger is at ([x], [y]). [PREVIOUS] or [NEXT] once, when the swipe has travelled far enough and is
     * horizontal enough; 0 before that and for the rest of the touch.
     */
    fun move(
        x: Float,
        y: Float,
    ): Int {
        val from = side ?: return 0
        if (done) return 0
        lastX = x
        lastY = y
        val inward = if (from == Side.LEFT) x - startX else startX - x
        if (inward < travelPx || abs(y - startY) > inward * MAX_SLOPE) return 0
        done = true
        return if (from == Side.LEFT) PREVIOUS else NEXT
    }

    /** The touch ended. */
    fun reset() {
        side = null
        done = false
    }

    companion object {
        const val PREVIOUS = -1
        const val NEXT = 1

        /** Vertical travel may be this share of the horizontal travel (about 31 degrees). */
        const val MAX_SLOPE = 0.6f
    }
}
