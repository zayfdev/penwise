package dev.penwise.ink

import kotlin.math.abs
import kotlin.math.hypot

/** One increment of a two-finger drag/pinch: the focal point moved `(fromX, fromY)` → `(toX, toY)`, span × [factor]. */
internal class NavStep(
    val factor: Float,
    val fromX: Float,
    val fromY: Float,
    val toX: Float,
    val toY: Float,
)

/**
 * Tells a two-finger *tap* from a two-finger *drag or pinch* (pure logic, no Android types). One finger never does
 * anything (it is usually the palm). Two fingers that stay within [slopPx] and lift within [tapTimeoutMs] are a tap
 * ([onTap]); as soon as either finger travels farther than the slop the session becomes navigation and reports
 * [NavStep]s (starting from where the fingers went down, so nothing is lost to the slop) and never fires a tap, however
 * the fingers lift. A third finger spoils the session. [onActive] brackets the navigation.
 */
internal class FingerNavigation(
    private val slopPx: Float,
    private val onTap: () -> Unit,
    private val onStep: (NavStep) -> Unit,
    private val onActive: (Boolean) -> Unit = {},
    private val tapTimeoutMs: Long = TAP_TIMEOUT_MS,
) {
    private val fingers = FingerSet()
    private var startTimeMs = 0L
    private var peak = 0
    private var spoiled = false
    private var navigating = false
    private var navigated = false
    private var prevX = 0f
    private var prevY = 0f
    private var prevSpan = 0f

    /** A finger touched down at `(x, y)`. */
    fun down(
        id: Int,
        x: Float,
        y: Float,
        timeMs: Long,
    ) {
        if (fingers.count == 0) {
            startTimeMs = timeMs
            peak = 0
            spoiled = false
            navigating = false
            navigated = false
        }
        if (fingers.indexOf(id) >= 0) return
        if (fingers.isFull) {
            spoil()
            return
        }
        fingers.add(id, x, y)
        peak = maxOf(peak, fingers.count)
        if (fingers.count > 2) spoil()
        if (fingers.count == 2 && !spoiled) rebase()
    }

    /** Records where finger [id] is now; call [update] after all pointers of one event were moved. */
    fun move(
        id: Int,
        x: Float,
        y: Float,
    ) {
        val i = fingers.indexOf(id)
        if (i >= 0) fingers.moveTo(i, x, y)
    }

    /** Evaluates the two fingers' new positions: starts navigation past the slop, then reports a step. */
    fun update() {
        if (spoiled || fingers.count != 2) return
        if (!navigating) {
            if (!fingers.movedBeyond(slopPx)) return
            navigating = true
            navigated = true
            onActive(true)
        }
        val span = fingers.span
        val x = fingers.midX
        val y = fingers.midY
        val factor = if (prevSpan > MIN_SPAN && span > MIN_SPAN) span / prevSpan else 1f
        onStep(NavStep(factor, prevX, prevY, x, y))
        prevX = x
        prevY = y
        prevSpan = span
    }

    /** Finger [id] lifted; the last one up ends the session (a tap if it qualifies). */
    fun up(
        id: Int,
        timeMs: Long,
    ) {
        val i = fingers.indexOf(id)
        if (i < 0) return
        drop(i)
        if (fingers.count > 0) return
        val tap = !spoiled && !navigated && peak == 2 && timeMs - startTimeMs <= tapTimeoutMs
        finish()
        if (tap) onTap()
    }

    /** Abandons the session (stylus, palm, system cancel): no tap, navigation stops. */
    fun cancel() {
        fingers.clear()
        finish()
    }

    /**
     * Drops every tracked finger except the ones [present] accepts (the pointers a touch event still reports): an
     * UP that was never delivered must not leave the session, and the canvas, stuck in a gesture.
     */
    fun retain(present: (Int) -> Boolean) {
        var i = 0
        while (i < fingers.count) {
            if (present(fingers.id(i))) i++ else drop(i)
        }
        if (fingers.count == 0) finish()
    }

    /**
     * Removes finger [index]. One finger never pans: the view freezes and the gesture ends; a finger that lands
     * again starts a new gesture from where the fingers are then.
     */
    private fun drop(index: Int) {
        fingers.remove(index)
        if (fingers.count == 2 && !spoiled) rebase()
        endNavigation()
    }

    private fun spoil() {
        spoiled = true
        endNavigation()
    }

    private fun endNavigation() {
        if (!navigating || (fingers.count == 2 && !spoiled)) return
        navigating = false
        onActive(false)
    }

    private fun finish() {
        val was = navigating
        navigating = false
        spoiled = false
        navigated = false
        peak = 0
        if (was) onActive(false)
    }

    private fun rebase() {
        prevX = fingers.midX
        prevY = fingers.midY
        prevSpan = fingers.span
        fingers.restart()
    }

    private companion object {
        const val TAP_TIMEOUT_MS = 300L
        const val MIN_SPAN = 1f
    }
}
