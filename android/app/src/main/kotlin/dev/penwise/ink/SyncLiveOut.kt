package dev.penwise.ink

/**
 * Forwards the pen's in-progress stroke to whichever [SyncClient] is current; the client is replaced on
 * reconfigure.
 */
internal class SyncLiveOut(
    private val client: () -> SyncClient,
) : LiveOut {
    override fun begin(
        preset: Int,
        argb: Int,
        scale: Float,
        seed: Long,
    ) = client().sendLiveBegin(preset, argb, scale, seed)

    override fun samples(
        samples: FloatArray,
        times: IntArray,
        count: Int,
    ) = client().sendLiveSamples(samples, times, count)

    override fun end(cancelled: Boolean) {
        if (cancelled) client().sendLiveCancel()
    }

    override fun shape(
        preset: Int,
        argb: Int,
        scale: Float,
        seed: Long,
        samples: FloatArray,
        times: IntArray,
        count: Int,
    ) = client().sendShapePreview(preset, argb, scale, seed, samples, times, count)

    override fun erasing(
        x: Float,
        y: Float,
        radius: Float,
    ) = client().sendErasePreview(x, y, radius)
}
