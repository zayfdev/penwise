package dev.penwise.ink

import kotlin.math.hypot

/**
 * Keeps the predicted tail short and in line with the pen. Extrapolation is wrong exactly where the pen turns, and
 * a long or sideways tail is ink that is drawn and then taken back at every corner; so the tail only follows the
 * direction the pen was heading and is cut off at [maxLength].
 */
internal object PredictionClip {
    /** Cosine of the widest angle between the pen's heading and a predicted point (about 30 degrees). */
    private const val MIN_COS = 0.85f

    private const val STRIDE = 5

    /**
     * Trims the first [count] predicted samples (`STRIDE` floats each, x then y first) in place and returns how many
     * to draw. The pen is at ([lastX], [lastY]) heading along ([headX], [headY]); a heading of almost no length
     * means the pen is not going anywhere yet, so nothing is predicted. The first point beyond [maxLength] from the
     * pen is pulled back onto that distance and ends the tail; the first point off the heading ends it before.
     */
    fun clip(
        predicted: FloatArray,
        count: Int,
        lastX: Float,
        lastY: Float,
        headX: Float,
        headY: Float,
        maxLength: Float,
    ): Int {
        val headLength = hypot(headX, headY)
        if (headLength < MIN_HEADING) return 0
        for (i in 0 until count) {
            val dx = predicted[i * STRIDE] - lastX
            val dy = predicted[i * STRIDE + 1] - lastY
            val length = hypot(dx, dy)
            if (length > 0f && (dx * headX + dy * headY) / (length * headLength) < MIN_COS) return i
            if (length > maxLength) {
                predicted[i * STRIDE] = lastX + dx * maxLength / length
                predicted[i * STRIDE + 1] = lastY + dy * maxLength / length
                return i + 1
            }
        }
        return count
    }

    private const val MIN_HEADING = 1e-3f
}
