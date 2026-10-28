package dev.penwise.ink

import android.graphics.Bitmap
import androidx.core.graphics.createBitmap
import java.nio.ByteBuffer
import kotlin.math.roundToInt

/**
 * Scratch for rasterizing one stamp stroke at a time: the coverage floats, their bytes and an `ALPHA_8` bitmap,
 * all reused between strokes. Not thread-safe (one per [GeometryPainter]).
 */
internal class StampBuffer {
    private var coverage = FloatArray(0)
    private var bytes = ByteArray(0)
    private var bitmap: Bitmap? = null

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
        StampRaster.accumulate(coverage, rect, pixelDabs, view, grain, heights)
        for (i in 0 until n) bytes[i] = (coverage[i] * BYTE_MAX).roundToInt().toByte()
        left = rect.left
        top = rect.top
        val target = bitmapFor(w, h)
        target.copyPixelsFromBuffer(ByteBuffer.wrap(bytes, 0, n))
        return target
    }

    private fun bitmapFor(
        w: Int,
        h: Int,
    ): Bitmap {
        val existing = bitmap
        if (existing != null && existing.allocationByteCount >= w * h) {
            existing.reconfigure(w, h, Bitmap.Config.ALPHA_8)
            return existing
        }
        return createBitmap(w, h, Bitmap.Config.ALPHA_8).also { bitmap = it }
    }

    private companion object {
        const val BYTE_MAX = 255f
    }
}
