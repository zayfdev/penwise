package dev.penwise.tools

import androidx.compose.ui.geometry.Offset
import kotlin.math.atan2
import kotlin.math.cos
import kotlin.math.hypot
import kotlin.math.sin

/** Where a touch on the wheel lands: the hue ring, the saturation/value square inside it, or neither. */
internal enum class WheelZone { RING, SQUARE, NONE }

/**
 * The colour wheel's layout in px: a hue ring [ringWidth] thick filling a [side] square, and a saturation/value
 * square of [squareSide] at its centre. Maps touches to hue, saturation and value, and back to thumb positions. Pure,
 * so it is unit-tested without a canvas.
 */
internal class WheelGeometry(
    val side: Float,
    val ringWidth: Float,
    val squareSide: Float,
) {
    private val centre = side / 2
    private val inner = centre - ringWidth
    private val squareLeft = centre - squareSide / 2

    /** Radius of the ring's centre line, where the hue thumb rides. */
    val ringRadius: Float get() = centre - ringWidth / 2

    /** Top-left corner of the saturation/value square. */
    val squareOrigin: Offset get() = Offset(squareLeft, squareLeft)

    fun zoneAt(
        x: Float,
        y: Float,
    ): WheelZone {
        val dx = x - centre
        val dy = y - centre
        val half = squareSide / 2
        return when {
            kotlin.math.abs(dx) <= half && kotlin.math.abs(dy) <= half -> WheelZone.SQUARE
            hypot(dx, dy) in inner..(centre + ringWidth / 2) -> WheelZone.RING
            else -> WheelZone.NONE
        }
    }

    /** Hue in degrees (0 until 360) of the touch's angle round the centre, clockwise from the right. */
    fun hueAt(
        x: Float,
        y: Float,
    ): Float {
        val degrees = Math.toDegrees(atan2(y - centre, x - centre).toDouble()).toFloat()
        return (degrees + FULL_TURN) % FULL_TURN
    }

    fun saturationAt(x: Float): Float = ((x - squareLeft) / squareSide).coerceIn(0f, 1f)

    fun valueAt(y: Float): Float = 1f - ((y - squareLeft) / squareSide).coerceIn(0f, 1f)

    /** Where the hue thumb sits for [hue]. */
    fun ringPoint(hue: Float): Offset {
        val radians = Math.toRadians(hue.toDouble())
        return Offset(centre + ringRadius * cos(radians).toFloat(), centre + ringRadius * sin(radians).toFloat())
    }

    /** Where the saturation/value thumb sits. */
    fun squarePoint(
        saturation: Float,
        value: Float,
    ): Offset = Offset(squareLeft + saturation * squareSide, squareLeft + (1f - value) * squareSide)

    private companion object {
        const val FULL_TURN = 360f
    }
}
