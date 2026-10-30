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

/**
 * Which frame is the live one (contract §3): the frame whose box covers the most of the screen, once its width on
 * screen is at least [ENTER_WIDTH] of the screen width. The live frame is kept until it drops under [EXIT_WIDTH] (or
 * leaves the screen) and a challenger must cover [SWITCH_MARGIN] times as much to take over, so a camera hovering at
 * the threshold does not flap between two WebViews. An explicit focus (a double-tap, a `Focus` message) is pinned: the
 * rule leaves it alone for as long as the frame stays on screen at [EXIT_WIDTH].
 *
 * Plain logic, free of Android types. UI thread only.
 */
class FocusRule {
    /** The live frame, or null (the board view: nothing is live). */
    var focused: String? = null
        private set

    private var pinned = false

    /** The owner or the agent chose [id] (null: nobody); it stays until the camera really leaves it. */
    fun explicit(id: String?) {
        focused = id
        pinned = id != null
    }

    /** Forgets the live frame (the page was left). */
    fun clear() = explicit(null)

    /** Re-evaluates the rule for the camera that has settled; returns the live frame afterwards. */
    fun evaluate(
        boxes: List<ScreenBox>,
        screenWidth: Float,
        screenHeight: Float,
    ): String? {
        if (screenWidth <= 0f || screenHeight <= 0f) return focused
        val current = boxes.firstOrNull { it.id == focused }
        val currentCover = current?.coverage(screenWidth, screenHeight) ?: 0f
        val stays = current != null && current.width >= EXIT_WIDTH * screenWidth && currentCover >= MIN_COVER
        if (pinned && stays) return focused
        pinned = false
        val best =
            boxes
                .filter { it.width >= ENTER_WIDTH * screenWidth }
                .map { it to it.coverage(screenWidth, screenHeight) }
                .filter { it.second >= MIN_COVER }
                .maxByOrNull { it.second }
        val keeps = stays && (best == null || best.first.id == focused || best.second <= currentCover * SWITCH_MARGIN)
        focused = if (keeps) focused else best?.first?.id
        return focused
    }

    companion object {
        /** A frame becomes the live one once it is this wide on screen (share of the screen width). */
        const val ENTER_WIDTH = 0.6f

        /** The live frame stays until it is narrower than this. */
        const val EXIT_WIDTH = 0.5f

        /** A challenger must cover this many times the live frame's share of the screen. */
        const val SWITCH_MARGIN = 1.25f

        /** A frame that covers less of the screen than this is off it for the rule's purposes. */
        const val MIN_COVER = 0.05f
    }
}
