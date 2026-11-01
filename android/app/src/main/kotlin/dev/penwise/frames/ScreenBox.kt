package dev.penwise.frames

import kotlin.math.max
import kotlin.math.min

/** A frame's box on the screen, in window px. */
data class ScreenBox(
    val id: String,
    val left: Float,
    val top: Float,
    val right: Float,
    val bottom: Float,
) {
    val width: Float get() = right - left

    /** The fraction of a [screenWidth] × [screenHeight] screen this box covers. */
    fun coverage(
        screenWidth: Float,
        screenHeight: Float,
    ): Float {
        val w = min(right, screenWidth) - max(left, 0f)
        val h = min(bottom, screenHeight) - max(top, 0f)
        return if (w <= 0f || h <= 0f) 0f else w * h / (screenWidth * screenHeight)
    }
}
