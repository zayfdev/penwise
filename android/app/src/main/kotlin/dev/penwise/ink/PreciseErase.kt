package dev.penwise.ink

import dev.penwise.core.Native
import kotlin.random.Random

/** What a precise erase did to one stroke. */
internal sealed interface EraseCut {
    /** The sweep left the stroke exactly as it was: nothing to replace. */
    data object Unchanged : EraseCut

    /** The stroke is replaced by [pieces]; empty when nothing survives. */
    class Pieces(
        val pieces: List<SavedStroke>,
    ) : EraseCut
}

/** Cuts strokes at an eraser sweep, through the same rule the engine uses (distance to the eraser path). */
internal object PreciseErase {
    /**
     * The result of cutting `stroke` at the sweep. A fragment keeps the brush, colour and size; its seed is
     * fresh so the mirror can address it as a new stroke.
     */
    fun cut(
        stroke: SavedStroke,
        path: FloatArray,
        pathCount: Int,
        radius: Float,
    ): EraseCut =
        decode(stroke, Native.preciseErase(stroke.samples, stroke.times, stroke.count, path, pathCount, radius))

    /** Decodes the native layout (see `Native.preciseErase`) into fragments of [stroke]'s brush. */
    fun decode(
        stroke: SavedStroke,
        flat: IntArray,
    ): EraseCut {
        val total = flat.firstOrNull() ?: return EraseCut.Unchanged
        if (total < 0) return EraseCut.Unchanged
        val out = ArrayList<SavedStroke>(total)
        var at = 1
        repeat(total) {
            val count = flat[at++]
            val samples = FloatArray(count * WIRE_STRIDE)
            val times = IntArray(count)
            for (i in 0 until count) {
                for (c in 0 until WIRE_STRIDE) samples[i * WIRE_STRIDE + c] = Float.fromBits(flat[at++])
                times[i] = flat[at++]
            }
            if (count >= 2) {
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
        }
        return EraseCut.Pieces(out)
    }

    private const val WIRE_STRIDE = 5
}
