package dev.penwise.ink

import android.view.MotionEvent
import androidx.input.motionprediction.MotionEventPredictor

/**
 * Routes stylus pointers to the gesture of the selected tool: ink, shape, erase or lasso. Each gesture maps the
 * stylus from screen to page space with the view it began under. Only
 * `TOOL_TYPE_STYLUS`/`TOOL_TYPE_ERASER` pointers draw; the pen is tracked by pointer id because a palm may own
 * pointer 0. Every gesture ends through [PenGesture.end] or [PenGesture.cancel], never silently: a system
 * `ACTION_CANCEL`, a cancelled pointer-up and [release] all abandon the gesture in flight.
 */
@Suppress("LongParameterList") // one value per collaborator of a pen gesture
internal class StylusStrokeController(
    scene: PageScene,
    private val brushes: BrushSource,
    live: LiveStroke,
    sink: StrokeSink,
    viewport: ViewportState,
    liveOut: LiveOut? = null,
    predictor: MotionEventPredictor? = null,
    frames: FrameBinder? = null,
) {
    private val env = GestureEnv(scene, brushes, live, sink, liveOut, predictor, viewport, frames)
    private var gesture: PenGesture? = null
    private var penId = NO_POINTER

    init {
        brushes.selection?.onDelete = ::deleteSelection
    }

    val isDrawing: Boolean get() = gesture != null

    /** Returns true when the event belongs to an active or starting pen gesture. */
    fun onTouchEvent(event: MotionEvent): Boolean {
        try {
            when (event.actionMasked) {
                MotionEvent.ACTION_DOWN, MotionEvent.ACTION_POINTER_DOWN -> begin(event)
                MotionEvent.ACTION_MOVE -> move(event)
                MotionEvent.ACTION_UP, MotionEvent.ACTION_POINTER_UP -> end(event)
                MotionEvent.ACTION_CANCEL -> cancel()
            }
        } catch (_: IllegalStateException) {
            // The page was closed under the pen (it was deleted or replaced mid-stroke): drop the stroke.
            gesture = null
            penId = NO_POINTER
            env.sink.cancelStroke()
        }
        return isDrawing || isStylus(event, event.actionIndex)
    }

    /** Abandons the gesture in flight (view detached, page replaced). */
    fun release() {
        cancel()
        brushes.selection?.onDelete = null
    }

    /** Abandons the gesture in flight but stays in use (the view was resized under the pen). */
    fun abandon() = cancel()

    private fun begin(event: MotionEvent) {
        if (gesture != null) return
        val index = event.actionIndex
        if (!isStylus(event, index)) return
        val id = event.getPointerId(index)
        gesture = start(event, index, id)
        penId = id
    }

    private fun start(
        event: MotionEvent,
        index: Int,
        pointerId: Int,
    ): PenGesture {
        val selection = brushes.selection
        if (brushes.lassoMode && selection != null) return SelectionGesture(env, selection, event, index)
        if (selection != null && selection.count > 0) {
            selection.clear()
            env.sink.cancelStroke()
        }
        val shape = brushes.shapeMode
        return when {
            brushes.eraserMode -> EraseGesture(env, event, index)
            shape != null -> ShapeGesture(env, shape, event, index)
            else -> InkGesture(env, pointerId, event, index)
        }
    }

    private fun move(event: MotionEvent) {
        val index = event.findPointerIndex(penId)
        if (index >= 0) gesture?.move(event, index)
    }

    private fun end(event: MotionEvent) {
        val index = event.actionIndex
        val current = gesture ?: return
        if (event.getPointerId(index) != penId) return
        gesture = null
        penId = NO_POINTER
        if (event.flags and MotionEvent.FLAG_CANCELED != 0) current.cancel() else current.end(event, index)
    }

    private fun cancel() {
        val current = gesture ?: return
        gesture = null
        penId = NO_POINTER
        current.cancel()
    }

    private fun deleteSelection() {
        val selection = brushes.selection ?: return
        if (selection.count == 0) return
        val scene = env.scene
        scene.beginGesture()
        try {
            scene.commit(scene.deleteItems(selection.ids))
        } finally {
            scene.endGesture()
        }
        selection.clear()
        env.sink.cancelStroke()
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
