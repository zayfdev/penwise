package dev.penwise.ink

/**
 * Collects live-preview samples between sends.
 *
 * The pen reports 100–200 samples a second and each used to become its own frame, so a burst queued frames the
 * mirror then replayed long after the pen had moved on. Samples are merged into one frame per tick; a frame
 * carries the index of its first sample so the receiver can place the chunk.
 */
internal class LiveBatcher {
    private var samples = FloatArray(INITIAL_SAMPLES * STRIDE)
    private var times = IntArray(INITIAL_SAMPLES)
    private var count = 0

    /** Index, within the stroke, of the first buffered sample. */
    var start: Int = 0
        private set

    /** True when there is something to send. */
    val pending: Boolean get() = count > 0

    /** Appends `sampleCount` interleaved samples (five floats each) and their times. */
    fun add(
        source: FloatArray,
        sourceTimes: IntArray,
        sampleCount: Int,
    ) {
        if (sampleCount <= 0) return
        ensureCapacity(count + sampleCount)
        source.copyInto(samples, count * STRIDE, 0, sampleCount * STRIDE)
        sourceTimes.copyInto(times, count, 0, sampleCount)
        count += sampleCount
    }

    /** Moves up to `max` buffered samples into the out arrays and returns how many; [start] advances past them. */
    fun take(
        outSamples: FloatArray,
        outTimes: IntArray,
        max: Int,
    ): Int {
        val taken = minOf(count, max, outTimes.size)
        if (taken == 0) return 0
        samples.copyInto(outSamples, 0, 0, taken * STRIDE)
        times.copyInto(outTimes, 0, 0, taken)
        samples.copyInto(samples, 0, taken * STRIDE, count * STRIDE)
        times.copyInto(times, 0, taken, count)
        count -= taken
        start += taken
        return taken
    }

    /** Starts a new stroke: drops everything and restarts the sample index. */
    fun reset() {
        count = 0
        start = 0
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
    }
}
