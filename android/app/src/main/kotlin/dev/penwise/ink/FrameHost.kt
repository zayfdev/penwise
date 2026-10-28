package dev.penwise.ink

import android.view.MotionEvent

/**
 * Live pages under the ink surface (the design harness' frames), as the canvas sees them: the surface hands every
 * finger sequence to the frames first and keeps the stylus for itself.
 */
interface FrameHost {
    /**
     * Routes one touch event that is not the pen's. True: the frames took it (a finger on a page); false: the canvas
     * gets it, as today. [handOver] is called when a gesture that began on a page became a two-finger canvas gesture:
     * the canvas must then take over every finger currently down, not just the new one.
     */
    fun route(
        event: MotionEvent,
        handOver: (MotionEvent) -> Unit,
    ): Boolean

    /** Abandons the gesture in flight (the pen came down); the page gets a cancel, so no stray click. */
    fun cancel()
}
