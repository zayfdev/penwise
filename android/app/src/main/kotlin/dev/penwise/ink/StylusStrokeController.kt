package dev.penwise.ink

import android.view.MotionEvent
import dev.penwise.core.InkStroke
import kotlin.random.Random

/** What the stroke controller needs from the renderer; all calls happen on the UI thread. */
internal interface StrokeSink {
    fun requestFrontFrame()

    fun commitStroke()

    fun cancelStroke()
}

/**
 * Turns stylus pointers into [InkStroke]s. Only `TOOL_TYPE_STYLUS`/`TOOL_TYPE_ERASER` pointers draw; the pen is
 * tracked by pointer id because a palm may own pointer 0.
 */
internal class StylusStrokeController(
    private val store: StrokeStore,
    private val brushes: BrushSource,
    private val live: LiveStroke,
    private val sink: StrokeSink,
) {
    private val buffer = SampleBuffer()
    private var stroke: InkStroke? = null
    private var penId = NO_POINTER
    private var startNanos = 0L
    private var strokeId = 0L

    val isDrawing: Boolean get() = stroke != null

    /** Returns true when the event belongs to an active or starting pen stroke. */
    fun onTouchEvent(event: MotionEvent): Boolean {
        when (event.actionMasked) {
            MotionEvent.ACTION_DOWN, MotionEvent.ACTION_POINTER_DOWN -> maybeBegin(event)
            MotionEvent.ACTION_MOVE -> move(event)
            MotionEvent.ACTION_UP, MotionEvent.ACTION_POINTER_UP -> maybeEnd(event)
            MotionEvent.ACTION_CANCEL -> cancel()
        }
        return isDrawing || isStylus(event, event.actionIndex)
    }

    fun release() {
        stroke?.close()
        stroke = null
        penId = NO_POINTER
    }

    private fun maybeBegin(event: MotionEvent) {
        val index = event.actionIndex
        if (stroke == null && isStylus(event, index)) {
            val brush = brushes.current()
            stroke = InkStroke(brush.presetIndex, Random.nextLong(), brush.argb, brush.sizeScale)
            penId = event.getPointerId(index)
            startNanos = event.eventTimeNanos
            strokeId++
            live.reset()
            append(event, index)
        }
    }

    private fun move(event: MotionEvent) {
        val index = if (stroke == null) -1 else event.findPointerIndex(penId)
        if (index >= 0) append(event, index)
    }

    private fun maybeEnd(event: MotionEvent) {
        val index = event.actionIndex
        if (stroke == null || event.getPointerId(index) != penId) return
        if (event.flags and MotionEvent.FLAG_CANCELED != 0) {
            cancel()
        } else {
            append(event, index)
            finish()
        }
    }

    private fun append(
        event: MotionEvent,
        index: Int,
    ) {
        val current = stroke ?: return
        buffer.fill(event, index, startNanos)
        current.push(buffer.samples, buffer.times, buffer.count)
        if (live.claimFrame()) {
            live.publish(LiveFrame(strokeId, current.snapshot()))
            sink.requestFrontFrame()
        }
    }

    private fun finish() {
        val current = stroke ?: return
        stroke = null
        penId = NO_POINTER
        val geometry = current.finish()
        current.close()
        store.add(geometry)
        sink.commitStroke()
    }

    private fun cancel() {
        if (stroke != null) {
            release()
            sink.cancelStroke()
        }
    }

    private fun isStylus(
        event: MotionEvent,
        index: Int,
    ): Boolean {
        val tool = event.getToolType(index)
        return tool == MotionEvent.TOOL_TYPE_STYLUS || tool == MotionEvent.TOOL_TYPE_ERASER
    }

    private companion object {
        const val NO_POINTER = -1
    }
}
