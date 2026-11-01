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
 * the window alone ([ScrollOwner]). After a pan or zoom of the board the page is scrolled so that its window covers
 * what is on screen.
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
        val viewport = camera()
        val byFinger = owner.byFinger(clock.now(), penActive())
        if (!isLive(target) || viewport == null || !byFinger || !target.moves) {
            place()
            return
        }
        target.needsAlign = false
        val scale = viewport.transform.scale
        val before = viewport.transform
        linking = true
        viewport.panBy(ScrollLink.cameraPan(dx, scale, true), ScrollLink.cameraPan(dy, scale, true))
        linking = false
        // The camera's own notification already placed the window; only a pan that changed nothing needs it here.
        if (viewport.transform == before) place()
    }

    /** Scrolls [target]'s page so its window covers the visible part of its frame (clamped to the document). */
    fun align(target: FollowTarget) {
        val scrollTo = coverTarget(target) ?: return
        target.scrollToDevicePx((scrollTo * target.pxPerCss).roundToInt())
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
}
