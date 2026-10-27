package dev.penwise.core

/**
 * One stroke being built by the native ink engine. Not thread-safe: use from one thread (or synchronize).
 *
 * @param presetIndex index into [InkPresets]
 * @param argb stroke color (`0xAARRGGBB`), overrides the preset color
 * @param sizeScale multiplies the preset's sizes
 */
class InkStroke(
    presetIndex: Int,
    seed: Long,
    private val argb: Int,
    sizeScale: Float,
) : AutoCloseable {
    private var handle: Long = Native.strokeNew(presetIndex, seed, argb, sizeScale)
    private val isStamps: Boolean
    private val opacity: Float
    private val multiply: Boolean
    private val grainScale: Float
    private val grainSoftness: Float

    init {
        var created = false
        val style =
            try {
                Native.strokeStyle(handle).also { created = true }
            } finally {
                if (!created) close()
            }
        isStamps = style[0] != 0f
        opacity = style[1]
        multiply = style[2] != 0f
        grainScale = style.getOrElse(GRAIN_SCALE_AT) { 0f }
        grainSoftness = style.getOrElse(GRAIN_SOFTNESS_AT) { 0f }
    }

    /**
     * Appends [count] samples: 5 floats each in [samples] (x, y, pressure 0..1, tilt rad, azimuth rad; view px)
     * and the matching microseconds since stroke start in [tUs].
     */
    fun push(
        samples: FloatArray,
        tUs: IntArray,
        count: Int,
    ) {
        require(count >= 0 && samples.size >= count * SAMPLE_FLOATS && tUs.size >= count) {
            "count $count exceeds samples (${samples.size}) or tUs (${tUs.size})"
        }
        Native.strokePush(liveHandle(), samples, tUs, count)
    }

    /** Empties the stroke (same brush and seed), keeping the native builder and its buffers for reuse. */
    fun reset() {
        Native.strokeReset(liveHandle())
    }

    /** Stable + tentative geometry so far. */
    fun snapshot(): InkGeometry {
        val h = liveHandle()
        return geometry(Native.strokeSnapshot(h), Native.strokeContourEnds(h))
    }

    /** Final geometry; the stroke is consumed and its native memory freed. */
    fun finish(): InkGeometry {
        val h = liveHandle()
        try {
            return geometry(Native.strokeFinish(h), Native.strokeContourEnds(h))
        } finally {
            close()
        }
    }

    override fun close() {
        val h = handle
        if (h != 0L) {
            handle = 0L
            Native.strokeFree(h)
        }
    }

    private fun liveHandle(): Long {
        check(handle != 0L) { "InkStroke already finished or closed" }
        return handle
    }

    private fun geometry(
        data: FloatArray,
        contourEnds: IntArray,
    ): InkGeometry =
        InkGeometry(
            isStamps = isStamps,
            outline = if (isStamps) EMPTY_FLOATS else data,
            contourEnds = contourEnds,
            dabs = if (isStamps) data else EMPTY_FLOATS,
            argb = argb,
            opacity = opacity,
            multiply = multiply,
            grainScale = grainScale,
            grainSoftness = grainSoftness,
        )

    private companion object {
        const val SAMPLE_FLOATS = 5
        const val GRAIN_SCALE_AT = 3
        const val GRAIN_SOFTNESS_AT = 4
        val EMPTY_FLOATS = FloatArray(0)
    }
}
