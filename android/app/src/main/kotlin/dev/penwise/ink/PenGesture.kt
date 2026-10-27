package dev.penwise.ink

import android.view.MotionEvent
import androidx.input.motionprediction.MotionEventPredictor

/** What the gestures need from the renderer; all calls happen on the UI thread. */
internal interface StrokeSink {
    fun requestFrontFrame()

    /** The front buffer's content is now part of the page: hand it over and clear the front buffer. */
    fun commitStroke()

    /** Clear the front buffer, nothing was committed. */
    fun cancelStroke()
}

/** What every gesture shares: the page it edits, the tool settings, and the way to the front buffer. */
internal class GestureEnv(
    val scene: PageScene,
    val brushes: BrushSource,
    val live: LiveStroke,
    val sink: StrokeSink,
    val liveOut: LiveOut?,
    val predictor: MotionEventPredictor?,
) {
    private var frames = 0L

    /** A fresh id for the next front-buffer frame sequence (the render thread clears when it changes). */
    fun nextFrameId(): Long = ++frames

    /** Shows [paint] on the front buffer now. */
    fun show(
        frameId: Long,
        paint: FrontPaint,
    ) {
        live.publish(LiveFrame(frameId, paint))
        sink.requestFrontFrame()
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
