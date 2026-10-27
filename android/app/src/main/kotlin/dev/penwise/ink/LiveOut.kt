package dev.penwise.ink

/**
 * Receives the stroke that is still under the pen, so the other devices can show it before the pen lifts.
 *
 * Called on the UI thread from the input controller; implementations must not block. The stroke ends with its
 * committed `Add` (same [id], which the server uses to drop the preview) or with [end] `cancelled = true`.
 */
interface LiveOut {
    /** Starts a preview of stroke [id] on [page]; subsequent [samples] calls are deltas from this point. */
    fun begin(
        page: ByteArray,
        id: ByteArray,
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

    /** The stroke was committed (nothing to send) or cancelled (the preview must be withdrawn). */
    fun end(cancelled: Boolean)
}
