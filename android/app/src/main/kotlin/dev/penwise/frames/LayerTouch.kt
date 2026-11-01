package dev.penwise.frames

import android.os.SystemClock
import android.view.View
import dev.penwise.ink.PenActivity
import dev.penwise.ink.ViewTransform
import kotlin.math.abs

/** The touch routing's view of the layer: where the frames are, the scroll link's finger events, double-tap, timers. */
internal class LayerTouch(
    private val lookup: FrameLookup,
    private val follower: WindowFollower,
    private val view: View,
    private val transform: () -> ViewTransform,
    private val onDoubleTap: (String) -> Unit,
    /** The locked frame's id; a still part scrolls the page only while its frame is the locked one. */
    private val locked: () -> String? = { null },
    private val pen: () -> PenActivity? = { null },
    private val edgeHold: () -> Boolean = { false },
) : GestureHost<FrameEntry> {
    override fun liveAt(
        x: Float,
        y: Float,
    ): FrameEntry? = lookup.liveAt(x, y)

    override fun frameAt(
        x: Float,
        y: Float,
    ): String? = lookup.specAt(x, y)?.id

    override fun scrollableAt(
        x: Float,
        y: Float,
    ): FrameEntry? = lookup.stillOfLiveAt(x, y, locked())

    override fun penRecent(time: Long): Boolean = pen()?.recent(time) == true

    override fun holdScroll(): Boolean = edgeHold()

    private val carry = ScrollCarry()

    override fun scrollPage(
        page: FrameEntry,
        dx: Float,
        dy: Float,
    ) {
        val (x, y) = carry.viewPx(dx, dy, page.placement.scale, page.view.bounds())
        // The View's own scroll: instant whatever the page's `scroll-behavior`, kept inside the document; the scroll
        // listener then carries the camera (a locked frame follows every scroll).
        if (x != 0 || y != 0) page.view.scrollBy(x, y)
    }

    override fun flingPage(
        page: FrameEntry,
        vx: Float,
        vy: Float,
    ) {
        carry.reset()
        val velocity = DragVelocity(vx, vy)
        if (velocity.isRest) {
            page.stopFling()
            return
        }
        val (x, y) = carry.flingView(vx, vy, page.placement.scale)
        page.view.flingScroll(x, y)
    }

    override fun isFitted(page: FrameEntry): Boolean =
        abs(page.spec.cssWidth * transform().scale - view.width) < FITTED_SLACK_PX

    override fun onDoubleTap(frameId: String) = onDoubleTap.invoke(frameId)

    override fun fingerDown() = follower.fingerDown()

    override fun fingerUp() = follower.fingerUp()

    override fun post(
        job: Runnable,
        delayMs: Long,
    ) {
        view.postDelayed(job, delayMs)
    }

    override fun remove(job: Runnable) {
        view.removeCallbacks(job)
    }

    override fun now(): Long = SystemClock.uptimeMillis()

    companion object {
        /** Frame width on screen within this of the screen width counts as fitted. */
        const val FITTED_SLACK_PX = 4f
    }
}
