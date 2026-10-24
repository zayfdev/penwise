package dev.penwise.ink

import android.graphics.Color
import android.graphics.RectF
import dev.penwise.core.InkGeometry
import kotlin.math.max
import kotlin.math.roundToInt

/** Field offsets inside one dab of `InkGeometry.dabs`. */
internal object Dab {
    const val STRIDE = 9
    const val X = 0
    const val Y = 1
    const val RADIUS_X = 2
    const val RADIUS_Y = 3
    const val ANGLE = 4
    const val OPACITY = 5
    const val HARDNESS = 6
    const val GRAIN_STRENGTH = 7
    const val GRAIN_LEVEL = 8
}

private const val MAX_ALPHA = 255f
private const val BOUNDS_PAD_PX = 2f

/** Converts a 0..1 opacity to a paint alpha. */
internal fun alpha255(opacity: Float): Int = (opacity * MAX_ALPHA).roundToInt().coerceIn(0, MAX_ALPHA.toInt())

/** Stroke-level alpha: color alpha times the stroke opacity cap. */
internal fun InkGeometry.strokeAlpha(): Int = alpha255(Color.alpha(argb) / MAX_ALPHA * opacity)

/** Writes the padded page-space bounds of [this] into [out]; leaves [out] empty for empty geometry. */
internal fun InkGeometry.boundsInto(out: RectF) {
    out.setEmpty()
    if (isStamps) {
        var i = 0
        while (i + Dab.STRIDE <= dabs.size) {
            val r = max(dabs[i + Dab.RADIUS_X], dabs[i + Dab.RADIUS_Y])
            out.union(dabs[i + Dab.X] - r, dabs[i + Dab.Y] - r, dabs[i + Dab.X] + r, dabs[i + Dab.Y] + r)
            i += Dab.STRIDE
        }
    } else if (outline.size >= 2) {
        // RectF.union(x, y) on an empty rect would include the origin, so seed with the first point.
        out.set(outline[0], outline[1], outline[0], outline[1])
        var i = 2
        while (i + 1 < outline.size) {
            out.union(outline[i], outline[i + 1])
            i += 2
        }
    }
    if (!out.isEmpty) out.inset(-BOUNDS_PAD_PX, -BOUNDS_PAD_PX)
}
