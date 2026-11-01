package dev.penwise.frames

import dev.penwise.ink.Clock
import dev.penwise.ink.ViewportState
import dev.penwise.ink.panBy
import kotlin.math.abs
import kotlin.math.max
import kotlin.math.min
import kotlin.math.roundToInt

/** The core's live window of frame [frameId] (board `[x, y, w, h]`) for the scroll [scrollX], [scrollY] it was at. */
internal class WindowUpdate(
    val frameId: String,
    val window: FloatArray,
    val scrollX: Float,
    val scrollY: Float,
)

/** Receives the WebView's scroll offset (device px) from inside its own scroll handling; primitives, no boxing. */
internal fun interface ScrollListener {
    fun onScroll(
        x: Int,
        y: Int,
    )
}

/** What the scroll link needs from the live page; the frame entry is the real one, tests use a fake. */
internal interface FollowTarget {
    val spec: FrameSpec

    /** The frame's window moves with the page's scroll (a full-length frame). */
    val moves: Boolean

    /** Device px per CSS px inside the page ([PagePixels.pxPerCss]): converts the WebView's scroll offsets. */
    val pxPerCss: Float

    /** The page's scroll offset in CSS px, as of its last scroll event. */
    var cssX: Float
    var cssY: Float

    /** The window should still be made to cover the visible part (a fresh or woken page). */
    var needsAlign: Boolean

    /** The document height in CSS px, at least the frame's box. */
    val documentHeight: Float

    fun scrollToDevicePx(y: Int)

    /** Ends the page's own fling. */
    fun stopFling()
}

/**
 * The scroll link of a full-length frame (contract §5). The live window sits where the page's own scroll puts it, read
 * in the same UI frame the WebView scrolls in. When the owner's finger (or its fling) scrolled the page the camera
 * pans by the same amount, so the page stays put on the board and the window slides over it; any other scroll moves
 * the window alone ([ScrollOwner]). While the camera is locked to a frame every scroll of its live page carries the
 * camera, except with the pen down (then it catches up when the pen lifts). After a pan or zoom of the board the
 * page is scrolled so that its window covers what is on screen.
 */
internal class WindowFollower(
    private val camera: () -> ViewportState?,
    private val screenHeight: () -> Int,
    private val isLive: (FollowTarget) -> Boolean,
    private val penActive: () -> Boolean,
    private val place: () -> Unit,
    private val clock: Clock,
) {
    private val owner = ScrollOwner()

    /** The camera is moving because the page scrolled, not because the owner moved it. */
    var linking = false
        private set

    /** A finger works the page, or its fling may still be going. */
    val engaged: Boolean get() = owner.engaged

    /** A finger went down on the live window. */
    fun fingerDown() = owner.fingerDown()

    /** The finger lifted or was taken by the canvas. */
    fun fingerUp() = owner.fingerUp(clock.now())

    /** The pen came down: a fling of [target] ends at once. */
    fun penDown(target: FollowTarget?) {
        if (owner.penDown(clock.now())) target?.stopFling()
    }

    /** The WebView of [target] scrolled to ([left], [top]) device px. */
    fun onScroll(
        target: FollowTarget,
        left: Int,
        top: Int,
    ) {
        val x = ScrollLink.cssOffset(left, target.pxPerCss)
        val y = ScrollLink.cssOffset(top, target.pxPerCss)
        val dx = x - target.cssX
        val dy = y - target.cssY
        target.cssX = x
        target.cssY = y
        if (echo.matches(top, clock.now(), AlignEcho.slack(target.pxPerCss))) {
            // The window is already where the camera wants it: the scroll `align` made carries nothing.
            place()
            return
        }
        val viewport = camera()
        debt.dropUnless(target, viewport?.lock)
        val pen = penActive()
        val byFinger = owner.byFinger(clock.now(), pen)
        // A locked frame is the screen (contract §3.1): every scroll of its live page carries the camera, not only
        // the owner's. The pen is the exception; what it missed is paid back when it lifts ([catchUp]).
        val locked = viewport?.locked == true && isLive(target)
        // Also with the camera flying or dipping: a pan now would be overwritten by the next frame.
        val steady = viewport?.let(::isSteady) == true
        when {
            locked && (pen || !steady) && !byFinger -> {
                debt.add(target, viewport?.lock, dx, dy)
                place()
            }

            viewport == null || !usable(target) || !(byFinger || locked) -> {
                place()
            }

            else -> {
                carry(target, viewport, dx, dy)
            }
        }
    }

    private fun usable(target: FollowTarget) = isLive(target) && target.moves

    private fun isSteady(viewport: ViewportState) = !viewport.animating && viewport.dip == 1f

    /** Pans the camera with the page's scroll ([dx], [dy] CSS px) and what it still owed. */
    private fun carry(
        target: FollowTarget,
        viewport: ViewportState,
        dx: Float,
        dy: Float,
    ) {
        val panX = dx + debt.x
        val panY = dy + debt.y
        debt.clear()
        target.needsAlign = false
        val scale = viewport.transform.scale
        val before = viewport.transform
        linking = true
        viewport.panBy(ScrollLink.cameraPan(panX, scale, true), ScrollLink.cameraPan(panY, scale, true))
        linking = false
        // The camera's own notification already placed the window; only a pan that changed nothing needs it here.
        if (viewport.transform == before) place()
    }

    private val echo = AlignEcho(ALIGN_EVENT_MS)

    /** Page scroll (CSS px) a locked camera has not followed yet because the pen was down. */
    private val debt = ScrollDebt()

    /** The camera still owes the live page a catch-up. */
    val owes: Boolean get() = debt.owes

    /**
     * Pays back the scroll the camera skipped while the pen was down, as one short flight at the same zoom. True while
     * the pen is still down (try again later); false once paid (or nothing was owed).
     */
    fun catchUp(): Boolean {
        val viewport = camera()
        debt.target?.takeIf(isLive).let { debt.dropUnless(it, viewport?.lock) }
        val busy = penActive() || viewport?.let(::isSteady) == false
        if (!owes) return false
        if (busy) return true
        val (dx, dy) = debt.x to debt.y
        debt.clear()
        if (viewport != null && viewport.locked) {
            val now = viewport.transform
            val panX = ScrollLink.cameraPan(dx, now.scale, true)
            val panY = ScrollLink.cameraPan(dy, now.scale, true)
            viewport.flyTo(viewport.clamped(now.copy(tx = now.tx + panX, ty = now.ty + panY)), CATCH_UP_MS)
        }
        return false
    }

    /** Scrolls [target]'s page so its window covers the visible part of its frame (clamped to the document). */
    fun align(target: FollowTarget) {
        val scrollTo = coverTarget(target) ?: return
        val top = (scrollTo * target.pxPerCss).roundToInt()
        echo.expect((target.cssY * target.pxPerCss).roundToInt(), top, clock.now())
        target.scrollToDevicePx(top)
        place()
    }

    /** The scroll (CSS px) the page should be at, or null when the window already covers what is on screen. */
    private fun coverTarget(target: FollowTarget): Float? {
        val viewport = camera()
        val spec = target.spec
        val height = screenHeight()
        if (viewport == null || !target.moves || height <= 0) return null
        val view = viewport.transform
        val box = spec.boxHeight.toFloat()
        val top = view.toPageY(0f) - spec.y
        val bottom = view.toPageY(height.toFloat()) - spec.y
        if (bottom <= 0f || top >= box) return null
        target.needsAlign = false
        val visibleTop = max(top, 0f)
        val visibleBottom = min(bottom, box)
        if (ScrollLink.covers(target.cssY, spec.cssHeight.toFloat(), visibleTop, visibleBottom)) return null
        val scroll = ScrollLink.coverScroll(visibleTop, visibleBottom, spec.cssHeight.toFloat(), target.documentHeight)
        return scroll.takeIf { abs(it - target.cssY) >= 1f }
    }

    private companion object {
        /** The catch-up flight after the pen lifts. */
        const val CATCH_UP_MS = 120L

        /** The scroll event of an `align` comes within this long, or it is not that one. */
        const val ALIGN_EVENT_MS = 100L
    }
}
