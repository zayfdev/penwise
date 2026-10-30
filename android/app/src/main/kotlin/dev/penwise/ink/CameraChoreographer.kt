package dev.penwise.ink

import android.view.Choreographer

/** Drives a [ViewportState] flight from the display's frame clock, on the UI thread. */
internal class CameraChoreographer(
    private val viewport: ViewportState,
) : CameraDriver {
    private val choreographer = Choreographer.getInstance()
    private var scheduled = false
    private val callback =
        Choreographer.FrameCallback { frameTimeNanos ->
            scheduled = false
            if (viewport.flight.step(frameTimeNanos / NANOS_PER_MS)) request()
        }

    override fun request() {
        if (scheduled) return
        scheduled = true
        choreographer.postFrameCallback(callback)
    }

    /** The canvas is going away: no frame callback may outlive it. */
    fun stop() {
        choreographer.removeFrameCallback(callback)
        scheduled = false
        viewport.flight.cancel()
    }

    private companion object {
        const val NANOS_PER_MS = 1_000_000L
    }
}
