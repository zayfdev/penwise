package dev.penwise.frames

import kotlin.math.hypot

/**
 * Recognises a one-finger double-tap on a frame (pure logic, no Android types): two taps, each shorter than
 * [tapTimeoutMs] and within [slopPx] of where the finger went down, on the same frame within [gapMs] of each other.
 * A finger that wanders off (a pan, a scroll) is no tap; a second finger spoils the sequence.
 */
class DoubleTap(
    private val slopPx: Float,
    private val tapTimeoutMs: Long = TAP_TIMEOUT_MS,
    private val gapMs: Long = GAP_MS,
) {
    private var frame: String? = null
    private var downX = 0f
    private var downY = 0f
    private var downAt = 0L
    private var moved = false
    private var spoiled = false
    private var lastTapFrame: String? = null
    private var lastTapAt = 0L
    private var lastTapX = 0f
    private var lastTapY = 0f

    /** One finger went down on [frame] (null: off every frame). */
    fun down(
        frame: String?,
        x: Float,
        y: Float,
        timeMs: Long,
    ) {
        this.frame = frame
        downX = x
        downY = y
        downAt = timeMs
        moved = false
        spoiled = false
    }

    /** The finger is now at ([x], [y]). */
    fun move(
        x: Float,
        y: Float,
    ) {
        if (hypot(x - downX, y - downY) > slopPx) moved = true
    }

    /** A second finger touched: this is not a tap. */
    fun spoil() {
        spoiled = true
        lastTapFrame = null
    }

    /** The finger lifted; the id of the frame that was just double-tapped, or null. */
    fun up(
        x: Float,
        y: Float,
        timeMs: Long,
    ): String? {
        val tapped = frame
        frame = null
        if (tapped == null || spoiled || moved || timeMs - downAt > tapTimeoutMs) {
            lastTapFrame = null
            return null
        }
        val second =
            lastTapFrame == tapped && timeMs - lastTapAt <= gapMs && hypot(x - lastTapX, y - lastTapY) <= slopPx * 2
        if (second) {
            lastTapFrame = null
            return tapped
        }
        lastTapFrame = tapped
        lastTapAt = timeMs
        lastTapX = x
        lastTapY = y
        return null
    }

    /** Drops the sequence (the pen came down, the page changed). */
    fun reset() {
        frame = null
        lastTapFrame = null
    }

    companion object {
        /** A touch longer than this is a press, not a tap. */
        const val TAP_TIMEOUT_MS = 250L

        /** The longest pause between the two taps of a double-tap. */
        const val GAP_MS = 350L
    }
}
