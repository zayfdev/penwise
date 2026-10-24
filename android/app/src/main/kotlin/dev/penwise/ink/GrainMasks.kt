package dev.penwise.ink

import android.graphics.Bitmap
import android.graphics.BitmapShader
import android.graphics.BlendMode
import android.graphics.Paint
import android.graphics.Shader
import androidx.core.graphics.createBitmap
import dev.penwise.core.PaperGrain
import java.nio.ByteBuffer
import kotlin.math.roundToInt

/**
 * Paper grain as `DST_IN` paints over a stroke layer. Mirrors core `grain_coverage`:
 * `lerp(1, smoothstep(1 − level − w, 1 − level + w, h), strength)` per page pixel, with `h` from `PaperGrain.bakeTile`.
 * Masks are cached per quantized (strength, level); shared by both render layers, hence synchronized.
 */
internal class GrainMasks {
    private var heights: ByteArray? = null
    private val cache = HashMap<Int, Paint?>()

    @Synchronized
    fun paint(
        strength: Float,
        level: Float,
    ): Paint? {
        val s = quantize(strength)
        val l = quantize(level)
        val key = s * (STEPS + 1) + l
        if (!cache.containsKey(key)) cache[key] = build(s.toFloat() / STEPS, l.toFloat() / STEPS)
        return cache[key]
    }

    private fun build(
        strength: Float,
        level: Float,
    ): Paint? {
        val tile = heights ?: PaperGrain.bakeTile(1f, TILE).also { heights = it }
        if (tile.size != TILE * TILE || strength <= 0f) return null
        val edge = 1f - level
        val mask = ByteArray(tile.size)
        for (i in tile.indices) {
            val h = (tile[i].toInt() and BYTE_MASK) / BYTE_MAX
            val coverage = 1f + (smoothstep(edge - SOFTNESS, edge + SOFTNESS, h) - 1f) * strength
            mask[i] = (coverage * BYTE_MAX).roundToInt().toByte()
        }
        val bitmap = createBitmap(TILE, TILE, Bitmap.Config.ALPHA_8)
        bitmap.copyPixelsFromBuffer(ByteBuffer.wrap(mask))
        return Paint().apply {
            shader = BitmapShader(bitmap, Shader.TileMode.REPEAT, Shader.TileMode.REPEAT)
            blendMode = BlendMode.DST_IN
        }
    }

    private fun quantize(v: Float): Int = (v.coerceIn(0f, 1f) * STEPS).roundToInt()

    private fun smoothstep(
        e0: Float,
        e1: Float,
        x: Float,
    ): Float {
        val t = ((x - e0) / (e1 - e0)).coerceIn(0f, 1f)
        return t * t * (THREE - 2f * t)
    }

    private companion object {
        /** One full paper period at scale 1 (`PAPER_PERIOD · PAPER_CELL_PX`), so the tile repeats seamlessly. */
        const val TILE = 512
        const val STEPS = 32

        /** Core keeps softness in BrushParams, which InkGeometry does not carry; a mid value of the presets. */
        const val SOFTNESS = 0.2f
        const val BYTE_MASK = 0xFF
        const val BYTE_MAX = 255f
        const val THREE = 3f
    }
}
