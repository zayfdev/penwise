package dev.penwise.ink

import dev.penwise.core.InkGeometry
import dev.penwise.core.InkStroke

/**
 * The stroke as *drawn* while the pen is down: every sample goes through the same core builder, with the same
 * quantization and smoothing, that builds the committed stroke, so the live ink is the finished ink: the core only
 * ever recomputes the short tentative tail of the stroke, and `finish` commits exactly what the last snapshot showed.
 */
internal class PreviewStroke {
    private var stroke: InkStroke? = null
    private var tailStroke: InkStroke? = null
    private var tailSamples = FloatArray(TAIL_INITIAL * STRIDE)
    private var tailTimes = IntArray(TAIL_INITIAL)
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
    }

    /** Pushes the samples of one event, in a single native call. */
    fun push(
        samples: FloatArray,
        times: IntArray,
        count: Int,
    ) {
        if (count == 0) return
        val target = stroke ?: InkStroke(presetIndex, seed, argb, scale).also { stroke = it }
        target.push(samples, times, count)
    }

    /** Geometry so far; null before the first sample. */
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
        val needed = count + 1
        if (tailTimes.size < needed) {
            tailSamples = FloatArray(needed * STRIDE)
            tailTimes = IntArray(needed)
        }
        last.copyInto(tailSamples, 0, 0, STRIDE)
        tailTimes[0] = lastTime
        predicted.copyInto(tailSamples, STRIDE, 0, count * STRIDE)
        predictedTimes.copyInto(tailTimes, 1, 0, count)
        // One builder for the whole gesture: reset per frame instead of allocating a native stroke each time.
        val builder =
            tailStroke?.also(InkStroke::reset)
                ?: InkStroke(presetIndex, seed, argb, scale).also { tailStroke = it }
        builder.push(tailSamples, tailTimes, needed)
        return builder.snapshot()
    }

    fun close() {
        stroke?.close()
        stroke = null
        tailStroke?.close()
        tailStroke = null
    }

    private companion object {
        const val STRIDE = 5
        const val TAIL_INITIAL = 16
    }
}
