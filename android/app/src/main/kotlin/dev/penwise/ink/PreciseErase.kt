package dev.penwise.ink

import dev.penwise.core.Native
import kotlin.random.Random

/** Cuts strokes at an eraser sweep, through the same rule the engine uses (distance to the eraser path). */
internal object PreciseErase {
    /**
     * The fragments `stroke` leaves after the sweep, or `null` when nothing survives. A fragment keeps the
     * brush, colour and size; its seed is fresh so the mirror can address it as a new stroke.
     */
    fun fragments(
        stroke: SavedStroke,
        path: FloatArray,
        pathCount: Int,
        radius: Float,
    ): List<SavedStroke>? {
        val runs = Native.preciseErase(stroke.samples, stroke.count, path, pathCount, radius)
        val total = runs.firstOrNull() ?: return null
        if (total == 0) return null
        val out = ArrayList<SavedStroke>(total)
        for (run in 0 until total) {
            val start = runs[1 + run * 2]
            val end = runs[2 + run * 2]
            val count = end - start
            if (count < 2) continue
            val samples = FloatArray(count * WIRE_STRIDE)
            val times = IntArray(count)
            stroke.samples.copyInto(samples, 0, start * WIRE_STRIDE, end * WIRE_STRIDE)
            stroke.times.copyInto(times, 0, start, end)
            out +=
                SavedStroke(
                    presetIndex = stroke.presetIndex,
                    argb = stroke.argb,
                    sizeScale = stroke.sizeScale,
                    seed = Random.nextLong(),
                    samples = samples,
                    times = times,
                    count = count,
                )
        }
        return out.takeIf { it.isNotEmpty() }
    }

    private const val WIRE_STRIDE = 5
}
