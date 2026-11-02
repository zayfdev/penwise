package dev.penwise.ink

import android.graphics.Bitmap
import androidx.core.graphics.createBitmap
import java.nio.ByteBuffer
import kotlin.math.roundToInt

/**
 * Scratch for rasterizing one stamp stroke at a time: the coverage floats, their bytes, the paper-height cache and
 * `ALPHA_8` bitmaps, all reused between strokes. Not thread-safe (one per [GeometryPainter]).
 *
 * A hardware canvas records `drawBitmap` and reads the pixels when it replays, after the draw call returned, so a
 * bitmap just drawn must not be rewritten for the next frame: [bitmaps] of them are used in turn (a painter on a
 * software canvas, which reads at once, needs one). [RECORDED] (0) gives every stroke a bitmap of its own, for a
 * display list that keeps all of them and replays them every frame.
 */
internal class StampBuffer(
    bitmaps: Int = 1,
) {
    private var coverage = FloatArray(0)
    private var bytes = ByteArray(0)
    private var paper = FloatArray(0)
    private val ring = arrayOfNulls<Bitmap>(bitmaps.coerceAtLeast(1))
    private val fresh = bitmaps == RECORDED
    private var next = 0

    /** Pixel position of the last bitmap's top-left corner. */
    var left = 0
        private set
    var top = 0
        private set

    /** Coverage of [dabs] (page space, seen through [view]) inside [clip] (screen px) as an alpha mask, or null. */
    fun rasterize(
        dabs: FloatArray,
        grain: GrainStyle?,
        clip: PixelRect,
        view: RasterView,
    ): Bitmap? {
        val pixelDabs = StampRaster.pixelDabs(dabs, view)
        val rect = StampRaster.rect(pixelDabs, clip) ?: return null
        val (w, h) = rect.width to rect.height
        val n = w * h
        if (coverage.size < n) {
            coverage = FloatArray(n)
            bytes = ByteArray(n)
        } else {
            coverage.fill(0f, 0, n)
        }
        val heights = grain?.let { g -> HeightSource { x, y -> PaperNoise.height(x, y, g.scale) } }
        paper = StampRaster.accumulate(coverage, rect, pixelDabs, view, grain, heights, paper) ?: paper
        for (i in 0 until n) bytes[i] = (coverage[i] * BYTE_MAX).roundToInt().toByte()
        left = rect.left
        top = rect.top
        val target = bitmapFor(w, h)
        target.copyPixelsFromBuffer(ByteBuffer.wrap(bytes, 0, n))
        return target
    }

    /** The next ring bitmap sized [w] x [h]; a too-small one is replaced, never recycled (it may be replaying). */
    private fun bitmapFor(
        w: Int,
        h: Int,
    ): Bitmap {
        if (fresh) return createBitmap(w, h, Bitmap.Config.ALPHA_8)
        val slot = next
        next = (next + 1) % ring.size
        val existing = ring[slot]
        if (existing != null && existing.allocationByteCount >= w * h) {
            existing.reconfigure(w, h, Bitmap.Config.ALPHA_8)
            return existing
        }
        return createBitmap(w, h, Bitmap.Config.ALPHA_8).also { ring[slot] = it }
    }

    companion object {
        /** Bitmaps a painter on the hardware front buffer rotates through: drawn, queued and being rewritten. */
        const val HARDWARE_RING = 3

        /** A new bitmap for every stroke: a recorded display list holds on to each one. */
        const val RECORDED = 0
        private const val BYTE_MAX = 255f
    }
}
