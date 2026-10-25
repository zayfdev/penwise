package dev.penwise.ink

/**
 * Collects live-preview samples between sends.
 *
 * The pen reports 100–200 samples a second and each used to become its own WebSocket frame, so a burst queued
 * frames the mirror then replayed long after the pen had moved on. Batches are merged into one frame per tick:
 * nothing is lost (the samples are cumulative for the preview) and the queue stays one frame deep.
 */
internal class LiveBatcher {
    private var samples = FloatArray(INITIAL_SAMPLES * STRIDE)
    private var times = IntArray(INITIAL_SAMPLES)
    private var count = 0

    /** Identifies the stroke the buffered samples belong to; `-1` when nothing is buffered. */
    var strokeId: Int = NO_STROKE
        private set

    /** True when there is something to send. */
    val pending: Boolean get() = count > 0

    /** Samples currently buffered. */
    val pendingSamples: Int get() = count

    /** Appends `sampleCount` interleaved samples for [id], starting a new batch when the stroke changes. */
    fun add(
        id: Int,
        source: FloatArray,
        sourceTimes: IntArray,
        sampleCount: Int,
    ) {
        if (sampleCount <= 0) return
        if (strokeId != id) reset()
        strokeId = id
        ensureCapacity(count + sampleCount)
        source.copyInto(samples, count * STRIDE, 0, sampleCount * STRIDE)
        sourceTimes.copyInto(times, count, 0, sampleCount)
        count += sampleCount
    }

    /** Takes the buffered samples; the batch is empty afterwards. */
    fun take(
        outSamples: FloatArray,
        outTimes: IntArray,
    ): Int {
        if (count == 0) return 0
        val taken = count
        samples.copyInto(outSamples, 0, 0, taken * STRIDE)
        times.copyInto(outTimes, 0, 0, taken)
        reset()
        return taken
    }

    /** Drops everything (stroke ended, page switched, connection reset). */
    fun reset() {
        count = 0
        strokeId = NO_STROKE
    }

    private fun ensureCapacity(needed: Int) {
        if (times.size < needed) {
            samples = samples.copyOf(needed * STRIDE)
            times = times.copyOf(needed)
        }
    }

    private companion object {
        const val STRIDE = 5
        const val INITIAL_SAMPLES = 256
        const val NO_STROKE = -1
    }
}
