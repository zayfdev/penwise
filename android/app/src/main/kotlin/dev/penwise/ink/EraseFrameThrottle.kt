package dev.penwise.ink

/**
 * Coalesces eraser positions for the mirror: at most one frame per [minIntervalMs] (one display frame), carrying
 * the newest point. Points that arrive inside the interval, including a motion event's history, are held and the
 * latest goes out with [flush] at the end of the gesture, so the sweep's final position is never lost.
 */
internal class EraseFrameThrottle(
    minIntervalMs: Long,
    private val send: (x: Float, y: Float) -> Unit,
) {
    private val gate = PreviewGate(minIntervalMs)
    private var pendingX = 0f
    private var pendingY = 0f
    private var pending = false

    fun point(
        x: Float,
        y: Float,
        nowMs: Long,
    ) {
        if (gate.allow(nowMs)) {
            pending = false
            send(x, y)
        } else {
            pendingX = x
            pendingY = y
            pending = true
        }
    }

    /** Sends the held point, if any. */
    fun flush() {
        if (pending) {
            pending = false
            send(pendingX, pendingY)
        }
    }

    /** Forgets everything for a new gesture. */
    fun reset() {
        gate.reset()
        pending = false
    }
}
