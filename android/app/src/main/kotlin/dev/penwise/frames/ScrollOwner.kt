package dev.penwise.frames

/**
 * Who a scroll of the live page belongs to. Only the owner's finger on the page, and the fling that goes on after the
 * finger lifts, carry the camera along (the content follows the finger); every other scroll (a reload restoring its
 * position, an anchor, `scrollIntoView`, a smooth-scroll library, the client's own `scrollTo`) moves only the window,
 * because the page stays fixed on the board (contract §1). The pen never carries the camera, and its touch ends a
 * fling.
 *
 * A fling exists only if the finger scrolled the page before it lifted (a tap opens none), its first scroll event
 * comes within [FLING_START_MS] of the lift, and each next one within [quietMs] of the one before. Plain logic over
 * explicit times.
 */
class ScrollOwner(
    private val quietMs: Long = QUIET_MS,
) {
    private var touching = false
    private var scrolledThisTouch = false
    private var lastByFinger = NEVER
    private var liftedAt = NEVER

    /** True while a finger works the page or a fling may still be going: the pen then has something to end. */
    val engaged: Boolean get() = touching || lastByFinger != NEVER

    /** A finger went down on the live window. */
    fun fingerDown() {
        touching = true
        scrolledThisTouch = false
    }

    /** The finger lifted (or was taken by the canvas) at [now]; a fling follows only if it had scrolled the page. */
    fun fingerUp(now: Long) {
        if (!touching) return
        touching = false
        if (scrolledThisTouch) {
            lastByFinger = now
            liftedAt = now
        } else {
            lastByFinger = NEVER
            liftedAt = NEVER
        }
    }

    /**
     * The pen came down at [now]: any fling is over. True when one may have been running, so the caller can stop the
     * page's own fling as well.
     */
    fun penDown(now: Long): Boolean {
        val flinging = !touching && lastByFinger != NEVER && now - lastByFinger <= quietMs
        touching = false
        lastByFinger = NEVER
        liftedAt = NEVER
        return flinging
    }

    /** A scroll event at [now]: true when it is the finger's or its fling's. [penActive]: the pen writes or hovers. */
    fun byFinger(
        now: Long,
        penActive: Boolean,
    ): Boolean {
        val mine =
            when {
                penActive -> {
                    false
                }

                touching -> {
                    scrolledThisTouch = true
                    true
                }

                else -> {
                    followsFling(now)
                }
            }
        if (mine) lastByFinger = now
        return mine
    }

    /** After the lift: does a scroll at [now] continue a fling? The first one must follow the lift closely. */
    private fun followsFling(now: Long): Boolean {
        if (lastByFinger == NEVER) return false
        val gap = if (liftedAt != NEVER) FLING_START_MS else quietMs
        val follows = now - lastByFinger <= gap
        if (!follows) lastByFinger = NEVER
        liftedAt = NEVER
        return follows
    }

    private companion object {
        /** A fling's scroll events come every frame; a longer silence means it has stopped. */
        const val QUIET_MS = 150L

        /** The fling's first scroll follows the lift by about a frame. */
        const val FLING_START_MS = 50L
        const val NEVER = Long.MIN_VALUE
    }
}
