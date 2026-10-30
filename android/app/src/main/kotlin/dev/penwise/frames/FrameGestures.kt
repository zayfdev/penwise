package dev.penwise.frames

import android.os.SystemClock
import android.view.InputDevice
import android.view.MotionEvent

/** A single-pointer event in a page's own coordinates, as a touchscreen finger would make it. */
internal object PageTouch {
    fun send(
        entry: FrameEntry,
        action: Int,
        time: Long,
        downTime: Long,
        x: Float,
        y: Float,
    ) {
        val copy = MotionEvent.obtain(downTime, time, action, x, y, 0)
        // A touchscreen finger, not SOURCE_UNKNOWN: the WebView reads the tool type to decide it is a finger, and the
        // page's pointer events should say so too.
        copy.source = InputDevice.SOURCE_TOUCHSCREEN
        entry.view.dispatchTouchEvent(copy)
        copy.recycle()
    }
}

/** Takes the touches of the live window from window coordinates into the WebView's own, and into its view tree. */
private val webViewSink =
    PageSink<FrameEntry> { entry, action, time, downTime, x, y ->
        val code =
            when (action) {
                TouchAction.DOWN -> MotionEvent.ACTION_DOWN
                TouchAction.MOVE -> MotionEvent.ACTION_MOVE
                TouchAction.UP -> MotionEvent.ACTION_UP
                TouchAction.CANCEL -> MotionEvent.ACTION_CANCEL
            }
        PageTouch.send(entry, code, time, downTime, entry.placement.toLocalX(x), entry.placement.toLocalY(y))
    }

/**
 * The MotionEvent side of the finger routing: the ink surface hands every finger event here first, and the decisions
 * are the [TouchRouter]'s. Input never reaches the WebViews through the view tree (the ink surface is above and takes
 * every touch), which is why the router's events are synthesised.
 */
internal class FrameGestures(
    host: GestureHost<FrameEntry>,
    touchSlopPx: Float,
) {
    private val router = TouchRouter(host, webViewSink, touchSlopPx)
    private var pointer = MotionEvent.INVALID_POINTER_ID

    /** A finger is working the live page right now. */
    val touching: Boolean get() = router.touching

    /** A finger gesture is held, forwarded or a tap is waiting: the pen has something to cancel. */
    val busy: Boolean get() = router.busy

    /** [FrameHost.route]: true when the page took the event. */
    fun route(
        event: MotionEvent,
        handOver: (MotionEvent) -> Unit,
    ): Boolean {
        val fingers = fingers(event)
        val action = event.actionMasked
        if (action == MotionEvent.ACTION_POINTER_DOWN || fingers > 1) {
            // A second finger takes the gesture: the page gets a cancel (no stray click), the canvas every finger down.
            if (router.takeForCanvas()) handOver(event)
            if (fingers > 1) return false
        }
        return when (action) {
            MotionEvent.ACTION_DOWN -> down(event, fingers)
            MotionEvent.ACTION_MOVE -> router.onMove(x(event), y(event), event.eventTime)
            MotionEvent.ACTION_UP -> router.onUp(x(event), y(event), event.eventTime)
            MotionEvent.ACTION_CANCEL -> router.onCancel()
            else -> false
        }
    }

    private fun down(
        event: MotionEvent,
        fingers: Int,
    ): Boolean {
        if (fingers != 1) return false
        pointer = event.getPointerId(0)
        return router.onDown(event.getX(0), event.getY(0), event.eventTime, event.downTime)
    }

    private fun x(event: MotionEvent): Float = event.getX(index(event))

    private fun y(event: MotionEvent): Float = event.getY(index(event))

    private fun index(event: MotionEvent): Int = event.findPointerIndex(pointer).coerceAtLeast(0)

    /** The pen came down or the page went: the page gets a cancel, no stray click; no tap sequence survives. */
    fun cancel() = router.penDown()

    /** Abandons a gesture on [entry] (it is being closed under the finger). */
    fun release(entry: FrameEntry) = router.release(entry)

    /** Back to idle without a cancel (the layer is gone). */
    fun reset() = router.reset()

    private companion object {
        /** The page's pointer types in this event: palms and canceled pointers never count. */
        fun fingers(event: MotionEvent): Int {
            var count = 0
            if (event.flags and MotionEvent.FLAG_CANCELED != 0) return 0
            for (i in 0 until event.pointerCount) {
                if (event.getToolType(i) == MotionEvent.TOOL_TYPE_FINGER) count++
            }
            return count
        }
    }
}
