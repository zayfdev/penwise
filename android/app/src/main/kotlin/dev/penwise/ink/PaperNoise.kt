package dev.penwise.ink

import kotlin.math.floor

/**
 * Paper grain height, a port of core `stamp::texture::paper_height`: two octaves of tileable value noise in page
 * space with a `SplitMix64` lattice. Same function as the server's and the browser's, so grain lands on the same
 * page pixels everywhere. The lattice is built once, lazily; nothing here touches native code.
 */
internal object PaperNoise {
    private const val PERIOD = 256
    private const val CELL_PX = 2f
    private const val SEED = 0x7061_7065_725F_7631L
    private const val MIX_1 = -0x40a7b892e31b1a47L // 0xBF58476D1CE4E5B9
    private const val MIX_2 = -0x6b2fb644ecceee15L // 0x94D049BB133111EB
    private const val LATTICE_UNIT = 1f / 16_777_216f
    private const val OCTAVES = 2
    private const val FINE_WEIGHT = 0.5f
    private const val WEIGHT_SUM = 1.5f

    private val lattices: Array<FloatArray> by lazy { Array(OCTAVES) { bake(it) } }

    /** Core `paper_height(x, y, scale)` in `[0, 1]`; 0.5 for a degenerate input. */
    fun height(
        x: Float,
        y: Float,
        scale: Float,
    ): Float {
        if (!(x.isFinite() && y.isFinite() && scale.isFinite()) || scale <= 0f) return FLAT
        val cell = CELL_PX * scale
        val coarse = octave(lattices[0], x, y, cell, 0)
        val fine = octave(lattices[1], x, y, cell * FINE_WEIGHT, 1)
        if (coarse.isNaN() || fine.isNaN()) return FLAT
        return ((coarse + FINE_WEIGHT * fine) / WEIGHT_SUM).coerceIn(0f, 1f)
    }

    /** One octave with lattice period `256 shl o` cells; NaN when the cell coordinates overflow. */
    private fun octave(
        table: FloatArray,
        x: Float,
        y: Float,
        cell: Float,
        o: Int,
    ): Float {
        val gx = x / cell
        val gy = y / cell
        if (!(gx.isFinite() && gy.isFinite())) return Float.NaN
        val fx = floor(gx)
        val fy = floor(gy)
        val tx = smooth(gx - fx)
        val ty = smooth(gy - fy)
        val period = (PERIOD shl o).toLong()
        val i0 = wrap(fx, period)
        val j0 = wrap(fy, period)
        val i1 = (i0 + 1) % period.toInt()
        val j1 = (j0 + 1) % period.toInt()
        val n = period.toInt()
        val top = lerp(table[i0 * n + j0], table[i1 * n + j0], tx)
        val bottom = lerp(table[i0 * n + j1], table[i1 * n + j1], tx)
        return lerp(top, bottom, ty)
    }

    private fun wrap(
        floored: Float,
        period: Long,
    ): Int = floored.toLong().mod(period).toInt()

    private fun smooth(t: Float): Float = t * t * (THREE - TWO * t)

    private fun lerp(
        a: Float,
        b: Float,
        t: Float,
    ): Float = a + (b - a) * t

    private fun bake(o: Int): FloatArray {
        val n = PERIOD shl o
        return FloatArray(n * n) { k ->
            val key = SEED xor (o.toLong() shl OCTAVE_SHIFT) xor ((k / n).toLong() shl CELL_SHIFT) xor (k % n).toLong()
            (mix64(key) ushr TOP_SHIFT).toInt() * LATTICE_UNIT
        }
    }

    private fun mix64(z0: Long): Long {
        val z1 = (z0 xor (z0 ushr 30)) * MIX_1
        val z2 = (z1 xor (z1 ushr 27)) * MIX_2
        return z2 xor (z2 ushr MIX_SHIFT_3)
    }

    private const val FLAT = 0.5f
    private const val TWO = 2f
    private const val THREE = 3f
    private const val MIX_SHIFT_3 = 31
    private const val OCTAVE_SHIFT = 62
    private const val CELL_SHIFT = 31
    private const val TOP_SHIFT = 40
}
