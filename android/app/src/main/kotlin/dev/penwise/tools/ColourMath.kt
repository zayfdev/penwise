package dev.penwise.tools

import kotlin.math.abs
import kotlin.math.floor
import kotlin.math.max
import kotlin.math.min

/** A colour as hue in degrees (0 until 360), saturation and value (both 0 to 1). */
internal data class Hsv(
    val hue: Float,
    val saturation: Float,
    val value: Float,
)

/** Conversions between the wheel's HSV and the RGB ints the pen writes with. Pure, so they are unit-tested. */
@Suppress("MagicNumber") // the six 60 degree sectors of the hue circle and the RGB byte shifts
internal object ColourMath {
    private const val DEGREES = 360f
    private const val SECTOR = 60f
    private const val CHANNEL_MAX = 255
    private const val RGB_MASK = 0xFFFFFF
    private const val BYTE_MASK = 0xFF
    private const val MIN_VISIBLE = 0.25f
    private const val GREY_RAMP_EDGE = 0xFFFFFF

    /** `0xRRGGBB` for [hsv]. */
    fun toRgb(hsv: Hsv): Int {
        val hue = ((hsv.hue % DEGREES) + DEGREES) % DEGREES / SECTOR
        val chroma = hsv.value * hsv.saturation
        val x = chroma * (1f - abs(hue % 2f - 1f))
        val m = hsv.value - chroma
        val (r, g, b) =
            when (floor(hue).toInt()) {
                0 -> Triple(chroma, x, 0f)
                1 -> Triple(x, chroma, 0f)
                2 -> Triple(0f, chroma, x)
                3 -> Triple(0f, x, chroma)
                4 -> Triple(x, 0f, chroma)
                else -> Triple(chroma, 0f, x)
            }
        return (channel(r + m) shl 16) or (channel(g + m) shl 8) or channel(b + m)
    }

    /** The wheel position of an RGB int (alpha is ignored). A grey has hue 0. */
    fun fromRgb(rgb: Int): Hsv {
        val r = ((rgb shr 16) and BYTE_MASK) / CHANNEL_MAX.toFloat()
        val g = ((rgb shr 8) and BYTE_MASK) / CHANNEL_MAX.toFloat()
        val b = (rgb and BYTE_MASK) / CHANNEL_MAX.toFloat()
        val high = max(r, max(g, b))
        val delta = high - min(r, min(g, b))
        val hue =
            when {
                delta <= 0f -> 0f
                high == r -> SECTOR * (((g - b) / delta + 6f) % 6f)
                high == g -> SECTOR * ((b - r) / delta + 2f)
                else -> SECTOR * ((r - g) / delta + 4f)
            }
        return Hsv(hue, if (high > 0f) delta / high else 0f, high)
    }

    /**
     * [from] with its hue turned to [hue] by a ring drag. A dark or grey colour has no hue to show (the default ink is
     * nearly black), so a ring drag that starts on one lifts it to full saturation and value first.
     */
    fun withHue(
        from: Hsv,
        hue: Float,
    ): Hsv = if (hasHue(from)) from.copy(hue = hue) else Hsv(hue, 1f, 1f)

    /**
     * Whether [hsv] shows a hue at all. Black, greys and the nearly black default ink don't: their hue reads as 0, so a
     * ring marker for them would sit on red and look like red is picked.
     */
    fun hasHue(hsv: Hsv): Boolean = hsv.saturation >= MIN_VISIBLE && hsv.value >= MIN_VISIBLE

    /** The right-hand edge of the square's gradient for [hsv]: its pure hue, or white's grey ramp without a hue. */
    fun squareEdge(
        hsv: Hsv,
        withHue: Boolean = hasHue(hsv),
    ): Int = if (withHue) toRgb(hsv.copy(saturation = 1f, value = 1f)) else GREY_RAMP_EDGE

    /** The saturation a square drag at [dragged] picks: a colour without a hue stays grey, so it stays at 0. */
    fun squareSaturation(
        dragged: Float,
        withHue: Boolean,
    ): Float = if (withHue) dragged else 0f

    /** `#RRGGBB`. */
    fun hex(rgb: Int): String = "#%06X".format(rgb and RGB_MASK)

    private fun channel(unit: Float): Int = (unit * CHANNEL_MAX + 0.5f).toInt().coerceIn(0, CHANNEL_MAX)
}
