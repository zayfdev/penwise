package dev.penwise.ink

import android.os.SystemClock
import android.view.MotionEvent
import kotlin.random.Random

/**
 * A freehand stroke. Every sample is kept for the commit (the core builds the final geometry, quantizes the
 * samples and stores them); the front buffer shows the same stroke built live by [PreviewStroke] from the same
 * samples, so what is drawn under the pen is what is committed at pen-up, plus a short predicted tail (clipped by
 * [PredictionClip]) when motion prediction is available. The other devices get the raw deltas through [LiveOut].
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

    /** The frame under the pen at pen-down; the element answer arrives while the stroke is drawn. */
    private val binding =
        env.frames?.beginBinding(event.getX(index), event.getY(index), env.brushes.bindToElement)
    private val startNanos = event.eventTimeNanos
    private val buffer = SampleBuffer()
    private val predicted = SampleBuffer()
    private val accumulated = StrokeAccumulator()
    private val view = env.viewport.transform
    private val preview = PreviewStroke()
    private val recentX = FloatArray(HEADING_SAMPLES)
    private val recentY = FloatArray(HEADING_SAMPLES)
    private var recentCount = 0
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
        val bindingJson = binding?.json()
        // A bound stroke needs the element's rect in the core first: the hit's rect is merged before the commit.
        if (bindingJson != null) binding?.prepare(scene)
        scene.beginGesture()
        try {
            val samples = accumulated.samplesCopy()
            val times = accumulated.timesCopy()
            val edits =
                if (bindingJson != null) {
                    scene.addBoundStroke(
                        id,
                        brush.presetIndex,
                        brush.argb,
                        brush.sizeScale,
                        seed,
                        samples,
                        times,
                        bindingJson,
                    )
                } else {
                    scene.addStroke(id, brush.presetIndex, brush.argb, brush.sizeScale, seed, samples, times)
                }
            scene.commit(edits)
        } finally {
            scene.endGesture()
            binding?.release()
        }
        env.liveOut?.end(cancelled = false)
        env.sink.commitStroke()
    }

    override fun cancel() {
        preview.close()
        binding?.release()
        env.liveOut?.end(cancelled = true)
        env.sink.cancelStroke()
    }

    private fun append(
        event: MotionEvent,
        index: Int,
    ) {
        buffer.fill(event, index, startNanos, view)
        accumulated.add(buffer.samples, buffer.times, buffer.count)
        preview.push(buffer.samples, buffer.times, buffer.count)
        trackHeading()
        env.liveOut?.samples(buffer.samples, buffer.times, buffer.count)
        // Order matters: `claimFrame` marks a render as in flight, and only a render clears it. Claiming a frame
        // the gate then declines leaves the flag stuck, and nothing is drawn again until the stroke ends.
        if (previewReady() && env.live.claimFrame()) {
            val geometry = preview.snapshot() ?: return
            val tail = if (StrokeFrame.hasTail(geometry)) predictedTail() else null
            env.live.publish(LiveFrame(frameId, StrokeFrame(geometry, tail)))
            env.sink.requestFrontFrame()
        }
    }

    /** Geometry of the predicted continuation of the pen, or null when the platform has no prediction. */
    private fun predictedTail(): dev.penwise.core.InkGeometry? {
        val event = env.predictor?.predict() ?: return null
        try {
            val index = event.findPointerIndex(penId)
            if (index < 0 || buffer.count == 0) return null
            predicted.fill(event, index, startNanos, view)
            val last = (buffer.count - 1) * SAMPLE_STRIDE
            val lastX = buffer.samples[last]
            val lastY = buffer.samples[last + 1]
            val oldest = if (recentCount < HEADING_SAMPLES) 0 else recentCount % HEADING_SAMPLES
            val keep =
                PredictionClip.clip(
                    predicted.samples,
                    predicted.count,
                    lastX,
                    lastY,
                    lastX - recentX[oldest],
                    lastY - recentY[oldest],
                    MAX_TAIL_PX / view.scale,
                )
            return preview.tail(
                buffer.samples.copyOfRange(last, last + SAMPLE_STRIDE),
                buffer.times[buffer.count - 1],
                predicted.samples,
                predicted.times,
                keep,
            )
        } finally {
            event.recycle()
        }
    }

    /** Remembers the newest pen positions: the oldest of them to the newest is the pen's heading. */
    private fun trackHeading() {
        for (i in 0 until buffer.count) {
            val slot = recentCount % HEADING_SAMPLES
            recentX[slot] = buffer.samples[i * SAMPLE_STRIDE]
            recentY[slot] = buffer.samples[i * SAMPLE_STRIDE + 1]
            recentCount++
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
        /** Longest predicted tail, screen px: a tail replaced every frame must not reach far past the pen. */
        const val MAX_TAIL_PX = 8f

        /** Samples (about 33 ms of pen travel at the digitizer rate) the heading is measured over. */
        const val HEADING_SAMPLES = 16

        /** Longest gap between preview geometry rebuilds, so a fast pen still shows motion. */
        const val PREVIEW_INTERVAL_MS = 12L
        const val SAMPLE_STRIDE = 5
    }
}
