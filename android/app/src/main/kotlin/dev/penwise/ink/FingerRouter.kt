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
                if (usable(event, index)) {
                    navigation.down(event.getPointerId(index), event.getX(index), event.getY(index), event.eventTime)
                }
            }

            MotionEvent.ACTION_MOVE -> {
                for (i in 0 until event.pointerCount) {
                    if (event.getToolType(i) == MotionEvent.TOOL_TYPE_FINGER) {
                        navigation.move(event.getPointerId(i), event.getX(i), event.getY(i))
                    }
                }
                navigation.update()
            }

            MotionEvent.ACTION_UP, MotionEvent.ACTION_POINTER_UP -> {
                if (event.getToolType(index) != MotionEvent.TOOL_TYPE_FINGER) return
                if (event.flags and MotionEvent.FLAG_CANCELED != 0) {
                    navigation.cancel()
                } else {
                    navigation.up(event.getPointerId(index), event.eventTime)
                }
            }

            MotionEvent.ACTION_CANCEL -> {
                navigation.cancel()
            }
        }
    }

    fun cancel() = navigation.cancel()

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
