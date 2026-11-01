package dev.penwise.frames

import android.view.MotionEvent

/** What the edge swipe needs of a touch event. */
internal enum class TouchKind { DOWN, MOVE, UP, CANCEL, OTHER }

/** One touch event reduced to what the [EdgeSwipeRouter] reads: the first pointer's place and how many there are. */
internal class EdgeTouch(
    val kind: TouchKind,
    val x: Float,
    val y: Float,
    val pointers: Int,
    /** The first pointer is a real finger (not a stylus, not a palm, not cancelled). */
    val finger: Boolean,
)

/** The edge-swipe view of [this] event. */
internal fun MotionEvent.toEdgeTouch(): EdgeTouch {
    val kind =
        when (actionMasked) {
            MotionEvent.ACTION_DOWN -> TouchKind.DOWN
            MotionEvent.ACTION_MOVE -> TouchKind.MOVE
            MotionEvent.ACTION_UP -> TouchKind.UP
            MotionEvent.ACTION_CANCEL -> TouchKind.CANCEL
            else -> TouchKind.OTHER
        }
    val finger = getToolType(0) == MotionEvent.TOOL_TYPE_FINGER && flags and MotionEvent.FLAG_CANCELED == 0
    return EdgeTouch(kind, getX(0), getY(0), pointerCount, finger)
}

/**
 * The touch side of the [EdgeSwipe]. A touch that starts in an edge strip (while [enabled]: the shown frame has a
 * neighbour in the switcher) goes to the page as ever while it is classified; once it is a horizontal edge swipe the
 * page gets a cancel ([cancelPage]), [onSwipe] fires and the router claims the rest of the touch. A second finger ends
 * the classification (the pinch belongs to the canvas). Everything else is not ours.
 */
internal class EdgeSwipeRouter(
    private val swipe: EdgeSwipe,
    private val enabled: () -> Boolean,
    private val screen: () -> Pair<Float, Float>,
    private val cancelPage: () -> Unit,
    private val onSwipe: (direction: Int) -> Unit,
) {
    private var claimed = false

    /** A touch that started in an edge strip leans sideways and is not yet an edge swipe: a still must not scroll. */
    val holdsScroll: Boolean get() = !claimed && swipe.leansHorizontal

    /** True while the router owns the rest of a touch. */
    val claiming: Boolean get() = claimed

    /** Routes [touch]: [page] is the page's own routing; true when the event was taken. */
    fun route(
        touch: EdgeTouch,
        page: () -> Boolean,
    ): Boolean {
        if (claimed) {
            if (touch.kind == TouchKind.UP || touch.kind == TouchKind.CANCEL) claimed = false
            return true
        }
        when (touch.kind) {
            TouchKind.DOWN -> down(touch)
            TouchKind.MOVE -> if (swipe.active && move(touch)) return true
            else -> swipe.reset()
        }
        return page()
    }

    private fun down(touch: EdgeTouch) {
        if (!touch.finger || touch.pointers != 1 || !enabled()) {
            swipe.reset()
            return
        }
        val (width, height) = screen()
        swipe.begin(touch.x, touch.y, width, height)
    }

    private fun move(touch: EdgeTouch): Boolean {
        if (touch.pointers != 1) {
            swipe.reset()
            return false
        }
        val direction = swipe.move(touch.x, touch.y)
        if (direction == 0) return false
        claimed = true
        cancelPage()
        onSwipe(direction)
        return true
    }
}
