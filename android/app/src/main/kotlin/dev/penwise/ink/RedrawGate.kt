package dev.penwise.ink

import java.util.concurrent.atomic.AtomicBoolean

/**
 * At most one page redraw in flight on the low-latency renderer. Its multi-buffered layer has three buffers; a flood of
 * redraws (a camera flight asks for one per frame) while the window's first frame is still being synced holds all
 * three, and the render thread then waits for a buffer for good: the ink freezes (Tab S9, a board opening fitted,
 * `BufferPool: Waiting for buffer`). A request made while one is in flight is kept and sent once that one landed.
 */
internal class RedrawGate(
    /** Sends one redraw to the renderer; false when there is no renderer to take it. */
    private val send: () -> Boolean,
) {
    private val inFlight = AtomicBoolean(false)
    private val again = AtomicBoolean(false)

    /** Asks for a redraw: sent now, or after the one in flight lands. UI thread. */
    fun request() {
        if (!inFlight.compareAndSet(false, true)) {
            again.set(true)
            return
        }
        if (!send()) inFlight.set(false)
    }

    /** A redraw reached the screen (render thread); true when a kept request should now be sent. */
    fun landed(): Boolean {
        inFlight.set(false)
        return again.getAndSet(false)
    }

    /** The surface or the renderer changed: nothing in flight will land, the next request goes out at once. */
    fun reset() {
        inFlight.set(false)
        again.set(false)
    }
}
