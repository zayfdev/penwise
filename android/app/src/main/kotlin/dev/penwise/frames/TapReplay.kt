package dev.penwise.frames

import kotlin.math.hypot

/**
 * Taps on a page that was not fitted. A tap waits here for a second one; if none comes within [DoubleTap.GAP_MS] the
 * page gets it after all (a press and a release where it was), so a lone tap is only late, never lost. A next touch
 * far from the waiting tap is a different tap, and the waiting one is delivered at once; a next touch close to it may
 * be the second tap of a double-tap, so the waiting tap is suspended until that touch says what it is ([settle]).
 */
internal class TapReplay<T : Any>(
    private val host: GestureHost<T>,
    private val sink: PageSink<T>,
    private val nearPx: Float,
) {
    private var page: T? = null
    private var x = 0f
    private var y = 0f
    private var suspended = false
    private val job = Runnable { deliver() }

    /** A tap is waiting (on its timer, or suspended behind a possible double-tap). */
    val pending: Boolean get() = page != null

    fun schedule(
        target: T,
        windowX: Float,
        windowY: Float,
    ) {
        page = target
        x = windowX
        y = windowY
        suspended = false
        host.post(job, DoubleTap.GAP_MS)
    }

    /** A finger went down at ([downX], [downY]). */
    fun onNextDown(
        downX: Float,
        downY: Float,
    ) {
        if (page == null || suspended) return
        if (hypot(downX - x, downY - y) <= nearPx) {
            host.remove(job)
            suspended = true
        } else {
            host.remove(job)
            deliver()
        }
    }

    /** The touch that suspended the waiting tap turned out to be a double-tap ([doubled]: drop it) or not. */
    fun settle(doubled: Boolean) {
        if (!suspended) return
        if (doubled) clear() else deliver()
    }

    /** Nothing is replayed. */
    fun cancel() {
        host.remove(job)
        clear()
    }

    /** Nothing is replayed if the waiting tap was on [target]. */
    fun cancel(target: T) {
        if (page === target) cancel()
    }

    private fun deliver() {
        val target = page ?: return
        clear()
        val now = host.now()
        sink.send(target, TouchAction.DOWN, now, now, x, y)
        sink.send(target, TouchAction.UP, now, now, x, y)
    }

    private fun clear() {
        page = null
        suspended = false
    }
}
