package dev.penwise.ink

import android.view.MotionEvent
import androidx.input.motionprediction.MotionEventPredictor
import java.util.concurrent.atomic.AtomicLong

/** What the gestures need from the renderer; all calls happen on the UI thread. */
internal interface StrokeSink {
    fun requestFrontFrame()

    /** The front buffer's content is now part of the page: hand it over and clear the front buffer. */
    fun commitStroke()

    /** Clear the front buffer, nothing was committed. */
    fun cancelStroke()
}

/** What every gesture shares: the page it edits, the tool settings, and the way to the front buffer. */
@Suppress("LongParameterList") // one value per collaborator, handed to every gesture
internal class GestureEnv(
    val scene: PageScene,
    val brushes: BrushSource,
    val live: LiveStroke,
    val sink: StrokeSink,
    val liveOut: LiveOut?,
    val predictor: MotionEventPredictor?,
    val viewport: ViewportState,
    val frames: FrameBinder? = null,
) {
    /** A fresh id for the next front-buffer frame sequence (the render thread clears when it changes). */
    fun nextFrameId(): Long = frameIds.incrementAndGet()

    /** Shows [paint] on the front buffer now. */
    fun show(
        frameId: Long,
        paint: FrontPaint,
    ) {
        live.publish(LiveFrame(frameId, paint))
        sink.requestFrontFrame()
    }

    private companion object {
        /**
         * Process-wide: the render callback outlives the controller (rebuilt on every page switch) and remembers the
         * last id it drew, so ids from two controllers must never collide.
         */
        val frameIds = AtomicLong()
    }
}

/**
 * One pen gesture from pen-down to pen-up. A gesture either ends ([end]) or is abandoned ([cancel]); abandoning
 * must leave nothing behind: no committed edit for ink, shapes and moves, a closed undo step for erasing, and an
 * empty front buffer.
 */
internal interface PenGesture {
    fun move(
        event: MotionEvent,
        index: Int,
    )

    fun end(
        event: MotionEvent,
        index: Int,
    )

    fun cancel()
}
