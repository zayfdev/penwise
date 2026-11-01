package dev.penwise.frames

import kotlin.math.hypot

/** A fling's velocity in screen px per second. */
internal class DragVelocity(
    val x: Float,
    val y: Float,
) {
    val isRest: Boolean get() = hypot(x, y) < MIN_FLING_PX_S

    companion object {
        val NONE = DragVelocity(0f, 0f)

        /** Below this a lift is a stop, not a fling. */
        const val MIN_FLING_PX_S = 100f
    }
}

/**
 * A one-finger drag that began on the still part of the locked frame (the frame box outside the live window): once the
 * finger leaves the touch slop it is a scroll of the live page, like a browser's. Pure logic over explicit times: the
 * moves it reports are screen-px deltas, the lift reports the velocity of the last [WINDOW_MS] (zero when the finger
 * rested before lifting).
 */
internal class StillDrag(
    private val slop: Float,
) {
    private var downX = 0f
    private var downY = 0f
    private var lastX = 0f
    private var lastY = 0f
    private var scrolling = false
    private val times = LongArray(SAMPLES)
    private val xs = FloatArray(SAMPLES)
    private val ys = FloatArray(SAMPLES)
    private var count = 0

    /** The finger moved past the slop: it scrolls the page now. */
    val active: Boolean get() = scrolling

    fun down(
        x: Float,
        y: Float,
        time: Long,
    ) {
        downX = x
        downY = y
        lastX = x
        lastY = y
        scrolling = false
        count = 0
        sample(x, y, time)
    }

    /** The finger is at ([x], [y]): the delta since its last point once it scrolls (from the down point), else null. */
    fun move(
        x: Float,
        y: Float,
        time: Long,
    ): Pair<Float, Float>? {
        sample(x, y, time)
        if (!scrolling) {
            if (hypot(x - downX, y - downY) <= slop) return null
            scrolling = true
        }
        val delta = (x - lastX) to (y - lastY)
        lastX = x
        lastY = y
        return delta
    }

    /** The finger lifted at [time]: how fast it was going. */
    fun up(time: Long): DragVelocity {
        if (!scrolling || count < 2) return DragVelocity.NONE
        val newest = (count - 1) % SAMPLES
        if (time - times[newest] > REST_MS) return DragVelocity.NONE
        var oldest = newest
        for (back in 1 until minOf(count, SAMPLES)) {
            val i = ((count - 1 - back) % SAMPLES + SAMPLES) % SAMPLES
            if (times[newest] - times[i] > WINDOW_MS) break
            oldest = i
        }
        val dt = (times[newest] - times[oldest]) / MS_PER_S
        if (dt <= 0f) return DragVelocity.NONE
        return DragVelocity((xs[newest] - xs[oldest]) / dt, (ys[newest] - ys[oldest]) / dt)
    }

    private fun sample(
        x: Float,
        y: Float,
        time: Long,
    ) {
        val i = count % SAMPLES
        times[i] = time
        xs[i] = x
        ys[i] = y
        count++
    }

    private companion object {
        const val SAMPLES = 8
        const val WINDOW_MS = 100L
        const val REST_MS = 60L
        const val MS_PER_S = 1000f
    }
}
