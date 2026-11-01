package dev.penwise.frames

import dev.penwise.ink.PageRect
import kotlin.math.max
import kotlin.math.min
import kotlin.math.roundToInt

/** Page scroll (CSS px) a locked camera has not followed yet: it belongs to one live page under one lock. */
internal class ScrollDebt {
    var x = 0f
        private set
    var y = 0f
        private set
    private var owner: FollowTarget? = null
    private var lock: PageRect? = null

    val owes: Boolean get() = x != 0f || y != 0f

    fun add(
        target: FollowTarget,
        under: PageRect?,
        dx: Float,
        dy: Float,
    ) {
        owner = target
        lock = under
        x += dx
        y += dy
    }

    /** Forgets it when it is not [target]'s under [under] any more (another live frame, another lock). */
    fun dropUnless(
        target: FollowTarget?,
        under: PageRect?,
    ) {
        if (owes && (owner !== target || lock != under)) clear()
    }

    fun clear() {
        x = 0f
        y = 0f
        owner = null
        lock = null
    }

    val target: FollowTarget? get() = owner
}

/** The scroll event an `align` expects back from the WebView, which may clamp it near the document's end. */
internal class AlignEcho(
    private val windowMs: Long,
) {
    private var from = 0
    private var wanted: Int? = null
    private var at = 0L

    fun expect(
        fromTop: Int,
        toTop: Int,
        now: Long,
    ) {
        from = fromTop
        wanted = toTop
        at = now
    }

    /**
     * Is a scroll to [top] at [now] that echo? Within [slack] px of the asked-for top, or between where the page was
     * and there (same direction, stopped short). Unrelated events leave the expectation standing until it expires.
     */
    fun matches(
        top: Int,
        now: Long,
        slack: Int,
    ): Boolean {
        val to = wanted ?: return false
        if (now - at > windowMs) {
            wanted = null
            return false
        }
        val own = top != from && top in (min(from, to) - slack)..(max(from, to) + slack)
        if (own) wanted = null
        return own
    }

    companion object {
        fun slack(pxPerCss: Float): Int = pxPerCss.roundToInt().coerceAtLeast(1)
    }
}
