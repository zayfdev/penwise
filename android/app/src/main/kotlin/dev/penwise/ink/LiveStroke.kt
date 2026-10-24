package dev.penwise.ink

import dev.penwise.core.InkGeometry
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicReference

/** Latest geometry of the in-progress stroke, handed from the UI thread to the render thread. */
class LiveFrame(
    val strokeId: Long,
    val geometry: InkGeometry,
)

/**
 * Coalesces front-buffer renders: the digitizer delivers ~482 events/s, but only one render request is in flight at a
 * time and it always draws the newest snapshot.
 */
internal class LiveStroke {
    private val latest = AtomicReference<LiveFrame?>(null)
    private val pending = AtomicBoolean(false)

    /** Resets the in-flight flag; a dropped request (commit/cancel) must not block the next stroke. */
    fun reset() {
        pending.set(false)
        latest.set(null)
    }

    /** True when the caller should snapshot and request a render; false while a render is still queued. */
    fun claimFrame(): Boolean = pending.compareAndSet(false, true)

    fun publish(frame: LiveFrame) {
        latest.set(frame)
    }

    /** Render thread: takes the newest frame and allows the next request. */
    fun take(): LiveFrame? {
        pending.set(false)
        return latest.get()
    }
}
