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
    private val ids = IntArray(MAX_FINGERS)
    private val xs = FloatArray(MAX_FINGERS)
    private val ys = FloatArray(MAX_FINGERS)
    private val startX = FloatArray(MAX_FINGERS)
    private val startY = FloatArray(MAX_FINGERS)
    private var count = 0
    private var startTimeMs = 0L
    private var peak = 0
    private var spoiled = false
    private var navigating = false
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
        if (count == 0) {
            startTimeMs = timeMs
            peak = 0
            spoiled = false
            navigating = false
        }
        if (indexOf(id) >= 0) return
        if (count == MAX_FINGERS) {
            spoiled = true
            return
        }
        ids[count] = id
        xs[count] = x
        ys[count] = y
        startX[count] = x
        startY[count] = y
        count++
        peak = maxOf(peak, count)
        if (count > 2) spoiled = true
        if (count == 2 && !spoiled) rebase()
    }

    /** Records where finger [id] is now; call [update] after all pointers of one event were moved. */
    fun move(
        id: Int,
        x: Float,
        y: Float,
    ) {
        val i = indexOf(id)
        if (i < 0) return
        xs[i] = x
        ys[i] = y
    }

    /** Evaluates the two fingers' new positions: starts navigation past the slop, then reports a step. */
    fun update() {
        if (spoiled || count != 2) return
        if (!navigating) {
            if (!moved()) return
            navigating = true
            onActive(true)
        }
        val span = span()
        val x = (xs[0] + xs[1]) / 2f
        val y = (ys[0] + ys[1]) / 2f
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
        val i = indexOf(id)
        if (i < 0) return
        remove(i)
        if (count > 0) return
        val tap = !spoiled && !navigating && peak == 2 && timeMs - startTimeMs <= tapTimeoutMs
        finish()
        if (tap) onTap()
    }

    /** Abandons the session (stylus, palm, system cancel): no tap, navigation stops. */
    fun cancel() {
        count = 0
        finish()
    }

    private fun finish() {
        val was = navigating
        navigating = false
        spoiled = false
        peak = 0
        if (was) onActive(false)
    }

    private fun rebase() {
        prevX = (xs[0] + xs[1]) / 2f
        prevY = (ys[0] + ys[1]) / 2f
        prevSpan = span()
    }

    private fun span(): Float = hypot(xs[0] - xs[1], ys[0] - ys[1])

    private fun moved(): Boolean {
        for (i in 0 until count) {
            if (abs(xs[i] - startX[i]) > slopPx || abs(ys[i] - startY[i]) > slopPx) return true
        }
        return false
    }

    private fun indexOf(id: Int): Int {
        for (i in 0 until count) if (ids[i] == id) return i
        return -1
    }

    private fun remove(index: Int) {
        for (i in index until count - 1) {
            ids[i] = ids[i + 1]
            xs[i] = xs[i + 1]
            ys[i] = ys[i + 1]
            startX[i] = startX[i + 1]
            startY[i] = startY[i + 1]
        }
        count--
        if (count == 2 && !spoiled) rebase()
    }

    private companion object {
        const val MAX_FINGERS = 10
        const val TAP_TIMEOUT_MS = 300L
        const val MIN_SPAN = 1f
    }
}
