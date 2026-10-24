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
 * Turns stylus pointers into [InkStroke]s, or into eraser drags when [BrushSource.eraserMode] is on. Only
 * `TOOL_TYPE_STYLUS`/`TOOL_TYPE_ERASER` pointers draw; the pen is tracked by pointer id because a palm may own
 * pointer 0. Finished strokes are handed to [StrokeStore], which persists them.
 */
internal class StylusStrokeController(
    private val store: StrokeStore,
    private val brushes: BrushSource,
    private val live: LiveStroke,
    private val sink: StrokeSink,
    private val liveOut: LiveOut? = null,
) {
    private val buffer = SampleBuffer()
    private var stroke: InkStroke? = null
    private var activeBrush: BrushSpec? = null
    private var activeSeed = 0L
    private var penId = NO_POINTER
    private var startNanos = 0L
    private var strokeId = 0L

    private var shapeStart: FloatArray? = null
    private var shapeTemp: InkStroke? = null
    private var erasing = false
    private var erasePoints = FloatArray(INITIAL_ERASE_POINTS * 2)
    private var eraseCount = 0

    val isDrawing: Boolean get() = stroke != null || shapeStart != null

    /** Returns true when the event belongs to an active or starting pen gesture. */
    fun onTouchEvent(event: MotionEvent): Boolean {
        when (event.actionMasked) {
            MotionEvent.ACTION_DOWN, MotionEvent.ACTION_POINTER_DOWN -> maybeBegin(event)
            MotionEvent.ACTION_MOVE -> move(event)
            MotionEvent.ACTION_UP, MotionEvent.ACTION_POINTER_UP -> maybeEnd(event)
            MotionEvent.ACTION_CANCEL -> cancel()
        }
        return isDrawing || erasing || isStylus(event, event.actionIndex)
    }

    fun release() {
        stroke?.close()
        stroke = null
        penId = NO_POINTER
        erasing = false
        eraseCount = 0
        shapeStart = null
        shapeTemp?.close()
        shapeTemp = null
    }

    private fun maybeBegin(event: MotionEvent) {
        if (stroke != null || erasing) return
        val index = event.actionIndex
        if (!isStylus(event, index)) return
        penId = event.getPointerId(index)
        if (brushes.eraserMode) {
            erasing = true
            eraseCount = 0
            addErasePoints(event, index)
            erase(persist = false)
            return
        }
        val brush = brushes.current()
        if (brushes.shapeMode != null) {
            shapeStart = floatArrayOf(event.getX(index), event.getY(index))
            publishShapePreview(event.getX(index), event.getY(index))
            return
        }
        activeBrush = brush
        activeSeed = Random.nextLong()
        stroke = InkStroke(brush.presetIndex, activeSeed, brush.argb, brush.sizeScale)
        startNanos = event.eventTimeNanos
        strokeId++
        live.reset()
        liveOut?.begin(brush.presetIndex, brush.argb, brush.sizeScale, activeSeed)
        append(event, index)
    }

    private fun move(event: MotionEvent) {
        val start = shapeStart
        if (start != null) {
            val index = event.findPointerIndex(penId)
            if (index >= 0) publishShapePreview(event.getX(index), event.getY(index))
            return
        }
        if (erasing) {
            val index = event.findPointerIndex(penId)
            if (index >= 0) {
                addErasePoints(event, index)
                erase(persist = false)
            }
            return
        }
        val index = if (stroke == null) -1 else event.findPointerIndex(penId)
        if (index >= 0) append(event, index)
    }

    private fun maybeEnd(event: MotionEvent) {
        val index = event.actionIndex
        if (event.getPointerId(index) != penId) return
        val canceled = event.flags and MotionEvent.FLAG_CANCELED != 0
        val start = shapeStart
        if (start != null) {
            shapeStart = null
            val temp = shapeTemp
            shapeTemp = null
            if (canceled || temp == null) {
                temp?.close()
                sink.cancelStroke()
            } else {
                val brush = brushes.current()
                val built =
                    ShapeSamples.build(
                        brushes.shapeMode ?: ShapeMode.Line,
                        start[0],
                        start[1],
                        event.getX(index),
                        event.getY(index),
                    )
                temp.close()
                if (built == null) {
                    sink.cancelStroke()
                } else {
                    commitShape(brush, built)
                }
            }
            penId = NO_POINTER
            return
        }
        if (erasing) {
            if (canceled) {
                release()
            } else {
                addErasePoints(event, index)
                erase(persist = false)
                store.commitErase()
                erasing = false
                eraseCount = 0
                penId = NO_POINTER
            }
            return
        }
        if (stroke == null) return
        if (canceled) {
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
        liveOut?.samples(buffer.samples, buffer.times, buffer.count)
        if (live.claimFrame()) {
            live.publish(LiveFrame(strokeId, current.snapshot()))
            sink.requestFrontFrame()
        }
    }

    private fun finish() {
        val current = stroke ?: return
        val brush = activeBrush ?: return
        stroke = null
        penId = NO_POINTER
        // Store exactly what the engine consumed, so a reloaded page rebuilds identical geometry.
        val saved =
            SavedStroke(
                presetIndex = brush.presetIndex,
                argb = brush.argb,
                sizeScale = brush.sizeScale,
                seed = activeSeed,
                samples = buffer.samples.copyOf(buffer.count * SAMPLE_STRIDE),
                times = buffer.times.copyOf(buffer.count),
                count = buffer.count,
            )
        val geometry = current.finish()
        current.close()
        saved.attachGeometry(geometry)
        store.add(saved)
        liveOut?.end(cancelled = false)
        sink.commitStroke()
    }

    /** Rebuilds the preview stroke for the current drag box (a fresh stroke, since a builder cannot shrink). */
    private fun publishShapePreview(
        x: Float,
        y: Float,
    ) {
        val start = shapeStart ?: return
        val mode = brushes.shapeMode ?: return
        val built = ShapeSamples.build(mode, start[0], start[1], x, y) ?: return
        shapeTemp?.close()
        val brush = brushes.current()
        val preview = InkStroke(brush.presetIndex, PREVIEW_SEED, brush.argb, brush.sizeScale)
        preview.push(built.first, built.second, built.second.size)
        shapeTemp = preview
        live.reset()
        live.publish(LiveFrame(++strokeId, preview.snapshot()))
        sink.requestFrontFrame()
    }

    private fun commitShape(
        brush: BrushSpec,
        built: Pair<FloatArray, IntArray>,
    ) {
        val (samples, times) = built
        val seed = Random.nextLong()
        val shape = InkStroke(brush.presetIndex, seed, brush.argb, brush.sizeScale)
        shape.push(samples, times, times.size)
        val saved = SavedStroke(brush.presetIndex, brush.argb, brush.sizeScale, seed, samples, times, times.size)
        val geometry = shape.finish()
        shape.close()
        saved.attachGeometry(geometry)
        store.add(saved)
        liveOut?.end(cancelled = false)
        sink.commitStroke()
    }

    private fun cancel() {
        if (stroke != null) {
            liveOut?.end(cancelled = true)
            release()
            sink.cancelStroke()
        }
        if (erasing) {
            store.commitErase()
            release()
        }
    }

    private fun addErasePoints(
        event: MotionEvent,
        index: Int,
    ) {
        val needed = (eraseCount + event.historySize + 1) * 2
        if (erasePoints.size < needed) erasePoints = erasePoints.copyOf(needed)
        for (h in 0 until event.historySize) {
            putErasePoint(event.getHistoricalX(index, h), event.getHistoricalY(index, h))
        }
        putErasePoint(event.getX(index), event.getY(index))
    }

    private fun putErasePoint(
        x: Float,
        y: Float,
    ) {
        erasePoints[eraseCount * 2] = x
        erasePoints[eraseCount * 2 + 1] = y
        eraseCount++
    }

    private fun erase(persist: Boolean) {
        store.eraseUnder(erasePoints, eraseCount, brushes.eraserRadiusPx, persist)
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
        const val SAMPLE_STRIDE = 5
        const val INITIAL_ERASE_POINTS = 64

        /** Preview strokes are thrown away, so their seed only has to be stable within one redraw. */
        const val PREVIEW_SEED = 1L
    }
}
