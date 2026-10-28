package dev.penwise.ink

import android.view.MotionEvent

/**
 * Feeds the finger pointers of touch events to a [FingerNavigation]. Palms (`TOOL_TYPE_PALM`, `FLAG_CANCELED`) and
 * anything that is not a finger are ignored; a stylus in the event, a pen gesture in flight or a hovering pen
 * cancels the session, so a hand resting near the pen never pans, zooms or undoes.
 */
internal class FingerRouter(
    private val navigation: FingerNavigation,
) {
    /** [penBusy]: the stylus is down, drawing or hovering. */
    fun onTouchEvent(
        event: MotionEvent,
        penBusy: Boolean,
    ) {
        if (penBusy || hasStylus(event)) {
            navigation.cancel()
            return
        }
        val index = event.actionIndex
        when (event.actionMasked) {
            MotionEvent.ACTION_DOWN, MotionEvent.ACTION_POINTER_DOWN -> {
                // A first finger down means the previous sequence is over, whatever its UP did (it may be lost).
                if (event.actionMasked == MotionEvent.ACTION_DOWN) navigation.cancel()
                if (usable(event, index)) {
                    navigation.down(event.getPointerId(index), event.getX(index), event.getY(index), event.eventTime)
                }
            }

            MotionEvent.ACTION_MOVE -> {
                navigation.retain { id -> event.findPointerIndex(id) >= 0 }
                for (i in 0 until event.pointerCount) {
                    if (event.getToolType(i) == MotionEvent.TOOL_TYPE_FINGER) {
                        navigation.move(event.getPointerId(i), event.getX(i), event.getY(i))
                    }
                }
                navigation.update()
            }

            MotionEvent.ACTION_UP, MotionEvent.ACTION_POINTER_UP -> {
                val finger = event.getToolType(index) == MotionEvent.TOOL_TYPE_FINGER
                if (finger && event.flags and MotionEvent.FLAG_CANCELED == 0) {
                    navigation.up(event.getPointerId(index), event.eventTime)
                } else {
                    navigation.cancel()
                }
                // The last pointer is up: nothing may stay tracked, even if an earlier UP never arrived.
                if (event.actionMasked == MotionEvent.ACTION_UP) navigation.cancel()
            }

            MotionEvent.ACTION_CANCEL -> {
                navigation.cancel()
            }
        }
    }

    fun cancel() = navigation.cancel()

    /**
     * Starts the canvas session from every finger already down. A gesture that began on a page (frames) became a
     * two-finger canvas gesture, and the canvas has seen none of those fingers yet; nothing that happened before is a
     * pan, so the slop starts from here.
     */
    fun adopt(event: MotionEvent) {
        navigation.cancel()
        for (i in 0 until event.pointerCount) {
            if (usable(event, i)) {
                navigation.down(event.getPointerId(i), event.getX(i), event.getY(i), event.eventTime)
            }
        }
    }

    private fun usable(
        event: MotionEvent,
        index: Int,
    ): Boolean =
        event.getToolType(index) == MotionEvent.TOOL_TYPE_FINGER && event.flags and MotionEvent.FLAG_CANCELED == 0

    private fun hasStylus(event: MotionEvent): Boolean {
        for (i in 0 until event.pointerCount) {
            val tool = event.getToolType(i)
            if (tool == MotionEvent.TOOL_TYPE_STYLUS || tool == MotionEvent.TOOL_TYPE_ERASER) return true
        }
        return false
    }
}
