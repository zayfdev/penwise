package dev.penwise.ink

import android.os.SystemClock
import android.view.MotionEvent
import kotlin.random.Random

/**
 * A freehand stroke. Every sample is kept for the commit (the core builds the final geometry, quantizes the
 * samples and stores them); the front buffer shows a thinned preview built by [PreviewStroke], plus the predicted
 * tail when motion prediction is available. The other devices get the raw deltas through [LiveOut].
 */
internal class InkGesture(
    private val env: GestureEnv,
    private val penId: Int,
    event: MotionEvent,
    index: Int,
) : PenGesture {
    private val brush = env.brushes.current()
    private val seed = Random.nextLong()
    private val id = env.scene.newItemId()
    private val frameId = env.nextFrameId()
    private val startNanos = event.eventTimeNanos
    private val buffer = SampleBuffer()
    private val predicted = SampleBuffer()
    private val accumulated = StrokeAccumulator()
    private val preview = PreviewStroke(PREVIEW_SPACING_PX)
    private var lastPreviewAtMs = 0L

    init {
        preview.begin(brush, seed)
        env.live.reset()
        env.liveOut?.begin(env.scene.page, id, brush.presetIndex, brush.argb, brush.sizeScale, seed)
        append(event, index)
    }

    override fun move(
        event: MotionEvent,
        index: Int,
    ) = append(event, index)

    override fun end(
        event: MotionEvent,
        index: Int,
    ) {
        append(event, index)
        preview.close()
        val scene = env.scene
        scene.beginGesture()
        try {
            val edits =
                scene.addStroke(
                    id,
                    brush.presetIndex,
                    brush.argb,
                    brush.sizeScale,
                    seed,
                    accumulated.samplesCopy(),
                    accumulated.timesCopy(),
                )
            scene.commit(edits)
        } finally {
            scene.endGesture()
        }
        env.liveOut?.end(cancelled = false)
        env.sink.commitStroke()
    }

    override fun cancel() {
        preview.close()
        env.liveOut?.end(cancelled = true)
        env.sink.cancelStroke()
    }

    private fun append(
        event: MotionEvent,
        index: Int,
    ) {
        buffer.fill(event, index, startNanos)
        accumulated.add(buffer.samples, buffer.times, buffer.count)
        preview.push(buffer.samples, buffer.times, buffer.count)
        env.liveOut?.samples(buffer.samples, buffer.times, buffer.count)
        // Order matters: `claimFrame` marks a render as in flight, and only a render clears it. Claiming a frame
        // the gate then declines leaves the flag stuck, and nothing is drawn again until the stroke ends.
        if (previewReady() && env.live.claimFrame()) {
            val geometry = preview.snapshot() ?: return
            env.live.publish(LiveFrame(frameId, StrokeFrame(geometry, predictedTail())))
            env.sink.requestFrontFrame()
        }
    }

    /** Geometry of the predicted continuation of the pen, or null when the platform has no prediction. */
    private fun predictedTail(): dev.penwise.core.InkGeometry? {
        val event = env.predictor?.predict() ?: return null
        try {
            val index = event.findPointerIndex(penId)
            if (index < 0 || buffer.count == 0) return null
            predicted.fill(event, index, startNanos)
            val last = (buffer.count - 1) * SAMPLE_STRIDE
            return preview.tail(
                buffer.samples.copyOfRange(last, last + SAMPLE_STRIDE),
                buffer.times[buffer.count - 1],
                predicted.samples,
                predicted.times,
                predicted.count,
            )
        } finally {
            event.recycle()
        }
    }

    /** True when enough time passed since the last preview geometry rebuild. */
    private fun previewReady(): Boolean {
        val now = SystemClock.uptimeMillis()
        if (now - lastPreviewAtMs < PREVIEW_INTERVAL_MS) return false
        lastPreviewAtMs = now
        return true
    }

    private companion object {
        /** Distance between preview points while the pen is down; the committed stroke keeps them all. */
        const val PREVIEW_SPACING_PX = 3.5f

        /** Longest gap between preview geometry rebuilds, so a fast pen still shows motion. */
        const val PREVIEW_INTERVAL_MS = 12L
        const val SAMPLE_STRIDE = 5
    }
}
