package dev.penwise.ink

import kotlin.math.abs
import kotlin.math.cos
import kotlin.math.max
import kotlin.math.sin
import kotlin.math.sqrt

/** Shape tool modes: drag from corner to corner (or end to end) instead of drawing freehand. */
enum class ShapeMode {
    Line,
    Rect,
    Ellipse,
    Arrow,
}

/**
 * Turns a shape drag into pen samples, so shapes go through the same engine, storage and sync as ink.
 *
 * Output is `samples` (x, y, pressure, tilt, azimuth per point) and `times` (µs since stroke start), ready for
 * [dev.penwise.core.InkStroke.push]. Pressure is steady, so shapes render at one width with round ends.
 */
internal object ShapeSamples {
    const val PRESSURE = 0.6f
    private const val STEP_PX = 4f
    private const val MICROS_PER_SAMPLE = 4000
    private const val ELLIPSE_SEGMENTS = 128
    private const val ARROW_BARB = 0.22f

    // / Barb offset as a fraction of the barb length (≈ 45° spread per side).
    private const val BARB_SPREAD = 0.55f

    /** `null` when the drag is too small to be a shape. */
    fun build(
        mode: ShapeMode,
        x0: Float,
        y0: Float,
        x1: Float,
        y1: Float,
    ): Pair<FloatArray, IntArray>? {
        val points =
            when (mode) {
                ShapeMode.Line -> listOf(x0 to y0, x1 to y1)
                ShapeMode.Arrow -> arrow(x0, y0, x1, y1)
                ShapeMode.Rect -> rect(x0, y0, x1, y1)
                ShapeMode.Ellipse -> ellipse(x0, y0, x1, y1)
            }
        if (points.size < 2) return null
        val samples = FloatArray(points.size * STRIDE)
        val times = IntArray(points.size)
        points.forEachIndexed { i, (x, y) ->
            val o = i * STRIDE
            samples[o + OFFSET_X] = x
            samples[o + OFFSET_Y] = y
            samples[o + OFFSET_PRESSURE] = PRESSURE
            samples[o + OFFSET_TILT] = 0f
            samples[o + OFFSET_AZIMUTH] = 0f
            times[i] = i * MICROS_PER_SAMPLE
        }
        return samples to times
    }

    private fun rect(
        x0: Float,
        y0: Float,
        x1: Float,
        y1: Float,
    ): List<Pair<Float, Float>> {
        val left = minOf(x0, x1)
        val right = maxOf(x0, x1)
        val top = minOf(y0, y1)
        val bottom = maxOf(y0, y1)
        val points = ArrayList<Pair<Float, Float>>()
        points += dense(left, top, right, top)
        points += dense(right, top, right, bottom)
        points += dense(right, bottom, left, bottom)
        points += dense(left, bottom, left, top)
        return points
    }

    private fun ellipse(
        x0: Float,
        y0: Float,
        x1: Float,
        y1: Float,
    ): List<Pair<Float, Float>> {
        val cx = (x0 + x1) / 2f
        val cy = (y0 + y1) / 2f
        val rx = abs(x1 - x0) / 2f
        val ry = abs(y1 - y0) / 2f
        if (rx <= 0f || ry <= 0f) return emptyList()
        return (0..ELLIPSE_SEGMENTS).map { i ->
            val angle = TAU * i / ELLIPSE_SEGMENTS
            (cx + rx * cos(angle).toFloat()) to (cy + ry * sin(angle).toFloat())
        }
    }

    private fun arrow(
        x0: Float,
        y0: Float,
        x1: Float,
        y1: Float,
    ): List<Pair<Float, Float>> {
        val shaft = dense(x0, y0, x1, y1)
        val length = length(x0, y0, x1, y1)
        if (length <= 0f) return shaft
        // Two barbs 45° off the shaft, built from the unit vector and its perpendicular (no trig needed).
        val ux = (x1 - x0) / length
        val uy = (y1 - y0) / length
        val barb = (length * ARROW_BARB).coerceAtMost(length * 0.5f)
        val backX = x1 - ux * barb
        val backY = y1 - uy * barb
        val offset = barb * BARB_SPREAD
        val left = (backX - uy * offset) to (backY + ux * offset)
        val right = (backX + uy * offset) to (backY - ux * offset)
        return shaft + dense(x1, y1, left.first, left.second) + dense(x1, y1, right.first, right.second)
    }

    /** Points every [STEP_PX] px along the segment, excluding the start and including the end. */
    private fun dense(
        ax: Float,
        ay: Float,
        bx: Float,
        by: Float,
    ): List<Pair<Float, Float>> {
        val length = length(ax, ay, bx, by)
        val steps = max(1, (length / STEP_PX).toInt() + 1)
        return (1..steps).map { i ->
            val t = i.toFloat() / steps
            (ax + (bx - ax) * t) to (ay + (by - ay) * t)
        }
    }

    private fun length(
        ax: Float,
        ay: Float,
        bx: Float,
        by: Float,
    ): Float {
        val dx = bx - ax
        val dy = by - ay
        return sqrt(dx * dx + dy * dy)
    }

    private const val STRIDE = 5
    private const val OFFSET_X = 0
    private const val OFFSET_Y = 1
    private const val OFFSET_PRESSURE = 2
    private const val OFFSET_TILT = 3
    private const val OFFSET_AZIMUTH = 4
    private const val TAU = (Math.PI * 2).toFloat()
}
