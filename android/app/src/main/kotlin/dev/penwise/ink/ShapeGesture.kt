package dev.penwise.ink

import android.view.MotionEvent
import kotlin.math.hypot

/**
 * Dragging out a shape. The front buffer shows a plain guide of the drag box; the shape itself (outline geometry,
 * frame, brush) is built by the core when the pen lifts, so what lands on the page is the core's, not the guide's.
 */
internal class ShapeGesture(
    private val env: GestureEnv,
    private val mode: ShapeMode,
    event: MotionEvent,
    index: Int,
) : PenGesture {
    private val brush = env.brushes.current()
    private val startX = event.getX(index)
    private val startY = event.getY(index)
    private val frameId = env.nextFrameId()

    init {
        env.live.reset()
        guide(startX, startY)
    }

    override fun move(
        event: MotionEvent,
        index: Int,
    ) = guide(event.getX(index), event.getY(index))

    override fun end(
        event: MotionEvent,
        index: Int,
    ) {
        val x = event.getX(index)
        val y = event.getY(index)
        if (hypot(x - startX, y - startY) < MIN_DRAG_PX) {
            env.sink.cancelStroke()
            return
        }
        val scene = env.scene
        scene.beginGesture()
        try {
            val style = ShapeStyle(mode, brush.presetIndex, brush.argb, brush.sizeScale)
            scene.commit(scene.addShape(scene.newItemId(), startX, startY, x, y, style))
        } finally {
            scene.endGesture()
        }
        env.sink.commitStroke()
    }

    override fun cancel() = env.sink.cancelStroke()

    private fun guide(
        x: Float,
        y: Float,
    ) = env.show(frameId, ShapeGuideFrame(mode, startX, startY, x, y, brush.argb, GUIDE_WIDTH_PX * brush.sizeScale))

    private companion object {
        const val MIN_DRAG_PX = 4f
        const val GUIDE_WIDTH_PX = 3f
    }
}
