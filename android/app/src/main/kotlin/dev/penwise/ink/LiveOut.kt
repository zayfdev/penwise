package dev.penwise.ink

/**
 * Receives the stroke that is still under the pen, so the PC mirror can show it before the pen lifts.
 *
 * Called on the UI thread from the input controller; implementations must not block. A stroke that is committed
 * or cancelled ends the preview.
 */
interface LiveOut {
    /** Starts a preview; subsequent [samples] calls are deltas from this point. */
    fun begin(
        preset: Int,
        argb: Int,
        scale: Float,
        seed: Long,
    )

    /** New samples since the last call (`samples` is five floats per sample, `times` one µs value each). */
    fun samples(
        samples: FloatArray,
        times: IntArray,
        count: Int,
    )

    /** The stroke was committed (the server ends the preview itself) or cancelled (it must be withdrawn). */
    fun end(cancelled: Boolean)
}
