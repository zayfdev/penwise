package dev.penwise.frames

/** What a one-finger scroll of the locked frame's still part needs from the frames. */
internal interface StillScrollHost<T : Any> {
    /**
     * The locked frame's live page when the window point is on its box but off its window (the still part): a finger
     * there scrolls the page like a browser's. Null elsewhere and while no frame is locked.
     */
    fun scrollableAt(
        x: Float,
        y: Float,
    ): T? = null

    /** A finger on the still part dragged [page] by ([dx], [dy]) screen px. */
    fun scrollPage(
        page: T,
        dx: Float,
        dy: Float,
    ) = Unit

    /** The finger lifted going ([vx], [vy]) screen px/s: the page flings on. A rest or a touch-down stops it. */
    fun flingPage(
        page: T,
        vx: Float,
        vy: Float,
    ) = Unit

    /** The pen was in use within the palm window before [time]: a finger now is a resting palm, not a scroll. */
    fun penRecent(time: Long): Boolean = false

    /** The touch began in an edge strip and leans sideways: it may be an edge swipe, so no scroll starts yet. */
    fun holdScroll(): Boolean = false

    /** A finger started or stopped working the live page (its scroll then carries the camera). */
    fun fingerDown()

    fun fingerUp()
}

/**
 * One finger on the still part of the locked frame (its box outside the live window): a tap until it leaves the touch
 * slop, then a scroll of the live page like a browser's, with a fling after the lift. The decision is [StillDrag]'s.
 */
internal class StillScroll<T : Any>(
    private val host: StillScrollHost<T>,
    slop: Float,
) {
    private val drag = StillDrag(slop)

    /** The page this finger would scroll; null while no such finger is down. */
    var page: T? = null
        private set

    /** The finger left the slop: the page scrolls with it. */
    val scrolling: Boolean get() = drag.active

    /** A finger went down at ([x], [y]); true when it is on a locked frame's still part. */
    fun down(
        x: Float,
        y: Float,
        time: Long,
    ): Boolean {
        val target = host.scrollableAt(x, y) ?: return false
        if (host.penRecent(time)) return false
        page = target
        drag.down(x, y, time)
        // A touch on the page ends the fling it was in, as a touch on a browser does.
        host.flingPage(target, 0f, 0f)
        return true
    }

    /** The finger moved; true once it scrolls the page. */
    fun move(
        x: Float,
        y: Float,
        time: Long,
    ): Boolean {
        val target = page ?: return false
        val was = drag.active
        // A touch in an edge strip that leans sideways waits to learn whether it is an edge swipe.
        if (!was && host.holdScroll()) return false
        val delta = drag.move(x, y, time) ?: return false
        if (!was) host.fingerDown()
        host.scrollPage(target, delta.first, delta.second)
        return true
    }

    /** The finger lifted: the page flings on with its speed. True when the finger had scrolled the page. */
    fun up(time: Long): Boolean {
        val target = page ?: return false
        val velocity = drag.up(time)
        val scrolled = drag.active
        if (scrolled) {
            host.flingPage(target, velocity.x, velocity.y)
            host.fingerUp()
        }
        page = null
        return scrolled
    }

    /** The touch ends without a fling (a second finger, the pen, a cancel). */
    fun drop() {
        if (drag.active) host.fingerUp()
        page = null
    }

    fun reset() {
        page = null
    }
}

/** Which frames' still parts scroll the page. */
internal object StillZone {
    /**
     * A finger at a point scrolls the live page when the page is shown, the point is off its window (the still part),
     * on its frame's box ([boxFrameId]: the frame under the point), and that frame is the locked one.
     */
    fun scrolls(
        shown: Boolean,
        onWindow: Boolean,
        boxFrameId: String?,
        liveId: String,
        lockedId: String?,
    ): Boolean = shown && !onWindow && boxFrameId == liveId && lockedId == liveId
}
