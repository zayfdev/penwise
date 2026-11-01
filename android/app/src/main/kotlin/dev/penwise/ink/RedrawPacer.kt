package dev.penwise.ink

import java.util.concurrent.atomic.AtomicBoolean

/**
 * Paces redraw requests to the display: however many arrive between two frames, one goes on to [forward] at the next
 * frame. A finger scroll pans the camera once per page scroll frame, and each pan asks for a redraw; passed straight
 * on, they filled the multi-buffered layer's three buffers (`BufferPool: Waiting for buffer`). The last request is
 * never dropped: a request made at or after a frame schedules the next one. Requests may come from any thread (a scene
 * changes on the sync thread, the catalog and image workers); [forward] runs where [nextFrame] runs its callback,
 * the UI thread's frame callback.
 */
internal class RedrawPacer(
    /** Runs [frame] once at the next display frame (`Choreographer.postFrameCallback`). */
    private val nextFrame: (frame: () -> Unit) -> Unit,
    /** Receives the paced request (the in-flight [RedrawGate]). */
    private val forward: () -> Unit,
) {
    private val waiting = AtomicBoolean(false)

    /** Asks for a redraw at the next frame; further asks before it are folded into it. */
    fun request() {
        if (!waiting.compareAndSet(false, true)) return
        var booked = false
        try {
            nextFrame(::onFrame)
            booked = true
        } finally {
            // A frame that could not be booked must not wedge every later request.
            if (!booked) waiting.set(false)
        }
    }

    private fun onFrame() {
        waiting.set(false)
        forward()
    }
}
