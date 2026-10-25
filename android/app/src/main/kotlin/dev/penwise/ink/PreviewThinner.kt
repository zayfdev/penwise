package dev.penwise.ink

import kotlin.math.abs

/**
 * Decides which samples the *preview* stroke keeps, so drawing cost stops growing with stroke length.
 *
 * The engine rebuilds a stroke's whole geometry for every preview frame. Pushing every sample of a long stroke
 * makes each frame slower than the last, and the front-buffered renderer starts dropping and clearing frames —
 * the stroke visibly stutters and flashes. The preview therefore keeps points roughly every [spacingPx] and the
 * committed stroke keeps them all, so the finished ink is exact and the live ink stays cheap.
 */
internal class PreviewThinner(
    private val spacingPx: Float,
) {
    private var lastX = Float.NaN
    private var lastY = Float.NaN

    /** True when a sample at (x, y) is far enough from the last kept one to join the preview. */
    fun keep(
        x: Float,
        y: Float,
    ): Boolean {
        if (lastX.isNaN() || abs(x - lastX) >= spacingPx || abs(y - lastY) >= spacingPx) {
            lastX = x
            lastY = y
            return true
        }
        return false
    }

    /** Forgets the last kept point, for the next stroke. */
    fun reset() {
        lastX = Float.NaN
        lastY = Float.NaN
    }
}
