package dev.penwise.ink

import android.graphics.Color
import android.graphics.RadialGradient
import android.graphics.Shader
import kotlin.math.roundToInt

/**
 * Alpha falloff masks for dabs, defined on the unit circle (the painter scales the canvas to the dab radii).
 * Solid up to `hardness`, then linear to transparent at the rim; hard dabs get no shader. Black: the stroke
 * color is applied to the whole stroke layer afterwards.
 */
internal class DabShaders {
    private val cache = arrayOfNulls<Shader>(LEVELS + 1)

    fun forHardness(hardness: Float): Shader? {
        val level = (hardness.coerceIn(0f, 1f) * LEVELS).roundToInt()
        return if (level >= LEVELS) null else cache[level] ?: create(level).also { cache[level] = it }
    }

    private fun create(level: Int): Shader {
        val solid = level.toFloat() / LEVELS
        return RadialGradient(
            0f,
            0f,
            1f,
            intArrayOf(Color.BLACK, Color.BLACK, Color.TRANSPARENT),
            floatArrayOf(0f, solid, 1f),
            Shader.TileMode.CLAMP,
        )
    }

    private companion object {
        const val LEVELS = 32
    }
}
