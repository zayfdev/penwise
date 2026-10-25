package dev.penwise.ink

/**
 * Whole-stroke sample storage.
 *
 * The input path hands over one motion event's samples at a time (that is what the ink engine consumes per
 * `push`, and what the mirror streams live), so anything that persists or syncs a stroke must accumulate them
 * first. Keeping only the last delta stores a single-sample stroke per pen-lift.
 */
internal class StrokeAccumulator {
    private var samples = FloatArray(INITIAL_SAMPLES * STRIDE)
    private var times = IntArray(INITIAL_SAMPLES)

    /** Samples collected since the last [reset]. */
    var count = 0
        private set

    /** Drops everything, for the next stroke. */
    fun reset() {
        count = 0
    }

    /** Appends `count` interleaved samples (`STRIDE` floats each) and their microsecond stamps. */
    fun add(
        sourceSamples: FloatArray,
        sourceTimes: IntArray,
        count: Int,
    ) {
        if (count <= 0) return
        require(sourceSamples.size >= count * STRIDE) { "sample buffer too small" }
        require(sourceTimes.size >= count) { "time buffer too small" }
        ensureCapacity(this.count + count)
        sourceSamples.copyInto(samples, this.count * STRIDE, 0, count * STRIDE)
        sourceTimes.copyInto(times, this.count, 0, count)
        this.count += count
    }

    /** Exactly the collected samples, `STRIDE` floats each. */
    fun samplesCopy(): FloatArray = samples.copyOf(count * STRIDE)

    /** Exactly the collected time stamps. */
    fun timesCopy(): IntArray = times.copyOf(count)

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
