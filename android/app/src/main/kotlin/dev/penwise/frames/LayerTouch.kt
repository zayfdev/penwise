package dev.penwise.frames

import android.os.SystemClock
import android.view.View
import dev.penwise.ink.ViewTransform
import kotlin.math.abs

/** The touch routing's view of the layer: where the frames are, the scroll link's finger events, double-tap, timers. */
internal class LayerTouch(
    private val lookup: FrameLookup,
    private val follower: WindowFollower,
    private val view: View,
    private val transform: () -> ViewTransform,
    private val onDoubleTap: (String) -> Unit,
) : GestureHost<FrameEntry> {
    override fun liveAt(
        x: Float,
        y: Float,
    ): FrameEntry? = lookup.liveAt(x, y)

    override fun frameAt(
        x: Float,
        y: Float,
    ): String? = lookup.specAt(x, y)?.id

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
