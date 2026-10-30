package dev.penwise.frames

/** Where the agent (or another device) wants the view: a page, and a frame on it (null: the page's content). */
data class MoveTarget(
    val page: String,
    val item: String?,
    /** What the chip calls it: a page title or a frame's route. */
    val label: String,
    /** A `Focus` also fits the item (or the page's content); a plain `SwitchPage` only changes the page. */
    val fit: Boolean = true,
)

/**
 * Camera moves that come from outside (a `SwitchPage`, a `Focus`) must not pull the view away from a hand that is
 * writing: within [QUIET_MS] of the owner's last pen stroke the move is held and offered as a chip; later it just
 * happens. Plain logic over explicit times, so the 10 s rule is tested without a clock.
 */
class AgentMoves {
    /** The held move behind the chip, or null. */
    var pending: MoveTarget? = null
        private set

    /** True when [target] may be applied at [nowMs]; otherwise it is held (replacing an older held move). */
    fun admit(
        target: MoveTarget,
        nowMs: Long,
        lastDrawMs: Long,
    ): Boolean {
        if (lastDrawMs <= 0L || nowMs - lastDrawMs >= QUIET_MS) {
            pending = null
            return true
        }
        pending = target
        return false
    }

    /** The chip was tapped: the held move, now to be applied. */
    fun take(): MoveTarget? = pending.also { pending = null }

    /** Drops the held move (the owner moved on, or the chip was dismissed). */
    fun dismiss() {
        pending = null
    }

    companion object {
        /** How long after the last pen stroke an outside move is held back. */
        const val QUIET_MS = 10_000L
    }
}
