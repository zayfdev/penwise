package dev.penwise.ink

import android.view.MotionEvent

/** Reusable sample arrays in the layout `InkStroke.push` expects; grows only when an event carries more history. */
internal class SampleBuffer {
    var samples = FloatArray(INITIAL_CAPACITY * STRIDE)
        private set
    var times = IntArray(INITIAL_CAPACITY)
        private set
    var count = 0
        private set

    /**
     * Fills the buffer with all historical samples of [pointerIndex] (oldest first) plus the current one, mapped from
     * screen to page space by [view].
     */
    fun fill(
        event: MotionEvent,
        pointerIndex: Int,
        startNanos: Long,
        view: ViewTransform,
    ) {
        val history = event.historySize
        ensureCapacity(history + 1)
        count = 0
        for (h in 0 until history) {
            put(
                view.toPageX(event.getHistoricalX(pointerIndex, h)),
                view.toPageY(event.getHistoricalY(pointerIndex, h)),
                event.getHistoricalPressure(pointerIndex, h),
                event.getHistoricalAxisValue(MotionEvent.AXIS_TILT, pointerIndex, h),
                event.getHistoricalAxisValue(MotionEvent.AXIS_ORIENTATION, pointerIndex, h),
                event.getHistoricalEventTimeNanos(h) - startNanos,
            )
        }
        put(
            view.toPageX(event.getX(pointerIndex)),
            view.toPageY(event.getY(pointerIndex)),
            event.getPressure(pointerIndex),
            event.getAxisValue(MotionEvent.AXIS_TILT, pointerIndex),
            event.getAxisValue(MotionEvent.AXIS_ORIENTATION, pointerIndex),
            event.eventTimeNanos - startNanos,
        )
    }

    private fun put(
        x: Float,
        y: Float,
        pressure: Float,
        tilt: Float,
        azimuth: Float,
        elapsedNanos: Long,
    ) {
        val o = count * STRIDE
        samples[o] = x
        samples[o + 1] = y
        samples[o + 2] = pressure.coerceIn(0f, 1f)
        samples[o + OFFSET_TILT] = tilt
        samples[o + OFFSET_AZIMUTH] = azimuth
        times[count] = (elapsedNanos / NANOS_PER_MICRO).coerceIn(0L, Int.MAX_VALUE.toLong()).toInt()
        count++
    }

    private fun ensureCapacity(needed: Int) {
        if (times.size < needed) {
            samples = FloatArray(needed * STRIDE)
            times = IntArray(needed)
        }
    }

    private companion object {
        const val STRIDE = 5
        const val OFFSET_TILT = 3
        const val OFFSET_AZIMUTH = 4
        const val INITIAL_CAPACITY = 64
        const val NANOS_PER_MICRO = 1_000L
    }
}
