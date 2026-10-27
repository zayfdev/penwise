package dev.penwise.ink

import dev.penwise.core.InkGeometry
import dev.penwise.core.InkStroke

/**
 * The stroke as *drawn* while the pen is down: samples thinned to one every few pixels (so each frame's geometry
 * rebuild stays cheap however long the stroke gets) and pushed once per motion event. The committed stroke keeps
 * every sample; the core builds its final geometry.
 */
internal class PreviewStroke(
    spacingPx: Float,
) {
    private val thinner = PreviewThinner(spacingPx)
    private var stroke: InkStroke? = null
    private var kept = FloatArray(INITIAL * STRIDE)
    private var keptTimes = IntArray(INITIAL)
    private var presetIndex = 0
    private var seed = 0L
    private var argb = 0
    private var scale = 1f

    fun begin(
        brush: BrushSpec,
        seed: Long,
    ) {
        close()
        presetIndex = brush.presetIndex
        argb = brush.argb
        scale = brush.sizeScale
        this.seed = seed
        thinner.reset()
    }

    /** Pushes the samples of one event that are far enough from the last kept one, in a single native call. */
    fun push(
        samples: FloatArray,
        times: IntArray,
        count: Int,
    ) {
        var n = 0
        for (i in 0 until count) {
            if (!thinner.keep(samples[i * STRIDE], samples[i * STRIDE + 1])) continue
            if (n == keptTimes.size) grow()
            samples.copyInto(kept, n * STRIDE, i * STRIDE, (i + 1) * STRIDE)
            keptTimes[n++] = times[i]
        }
        if (n == 0) return
        val target = stroke ?: InkStroke(presetIndex, seed, argb, scale).also { stroke = it }
        target.push(kept, keptTimes, n)
    }

    /** Geometry so far; null before the first kept sample. */
    fun snapshot(): InkGeometry? = stroke?.snapshot()

    /**
     * Geometry of a throwaway stroke through the last real sample and the [predicted] ones: the erasable tail that
     * hides input latency. It never becomes part of the stroke.
     */
    fun tail(
        last: FloatArray,
        lastTime: Int,
        predicted: FloatArray,
        predictedTimes: IntArray,
        count: Int,
    ): InkGeometry? {
        if (count == 0) return null
        val samples = FloatArray((count + 1) * STRIDE)
        val times = IntArray(count + 1)
        last.copyInto(samples, 0, 0, STRIDE)
        times[0] = lastTime
        predicted.copyInto(samples, STRIDE, 0, count * STRIDE)
        predictedTimes.copyInto(times, 1, 0, count)
        val tail = InkStroke(presetIndex, seed, argb, scale)
        try {
            tail.push(samples, times, count + 1)
            return tail.snapshot()
        } finally {
            tail.close()
        }
    }

    fun close() {
        stroke?.close()
        stroke = null
    }

    private fun grow() {
        kept = kept.copyOf(kept.size * 2)
        keptTimes = keptTimes.copyOf(keptTimes.size * 2)
    }

    private companion object {
        const val STRIDE = 5
        const val INITIAL = 64
    }
}
