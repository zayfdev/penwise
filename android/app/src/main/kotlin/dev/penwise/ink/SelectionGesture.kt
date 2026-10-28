package dev.penwise.ink

import android.graphics.RectF
import android.view.MotionEvent

/**
 * The lasso tool. A pen-down outside the current selection draws a loop and selects what the core finds inside it;
 * a pen-down inside the selection drags it, and the move is one core edit when the pen lifts. The front buffer shows
 * the loop, then a frame around the selection (shifted while dragging).
 */
internal class SelectionGesture(
    private val env: GestureEnv,
    private val selection: Selection,
    event: MotionEvent,
    index: Int,
) : PenGesture {
    private val view = env.viewport.transform
    private val startX = view.toPageX(event.getX(index))
    private val startY = view.toPageY(event.getY(index))
    private val frameId = env.nextFrameId()
    private val dragging =
        selection.count > 0 &&
            selection.box.let { hitsSelection(it.left, it.top, it.right, it.bottom, startX, startY, view.scale) }
    private var loop = FloatArray(INITIAL_POINTS * 2)
    private var loopCount = 0
    private var dx = 0f
    private var dy = 0f

    init {
        env.live.reset()
        if (!dragging) {
            selection.clear()
            addPoints(event, index)
            show()
        }
    }

    override fun move(
        event: MotionEvent,
        index: Int,
    ) {
        if (dragging) {
            dx = view.toPageX(event.getX(index)) - startX
            dy = view.toPageY(event.getY(index)) - startY
        } else {
            addPoints(event, index)
        }
        show()
    }

    override fun end(
        event: MotionEvent,
        index: Int,
    ) {
        if (dragging) {
            dx = view.toPageX(event.getX(index)) - startX
            dy = view.toPageY(event.getY(index)) - startY
            drop()
        } else {
            addPoints(event, index)
            select()
        }
    }

    override fun cancel() {
        dx = 0f
        dy = 0f
        if (dragging) show() else env.sink.cancelStroke()
    }

    private fun select() {
        val ids =
            if (loopCount < MIN_LOOP_POINTS) {
                ByteArray(0)
            } else {
                env.scene.lasso(loop.copyOf(loopCount * 2), Layers.ALL)
            }
        selection.select(ids)
        if (ids.isEmpty()) {
            env.sink.cancelStroke()
            return
        }
        selection.box.set(boundsOf(ids))
        show()
    }

    private fun drop() {
        val scene = env.scene
        if (kotlin.math.abs(dx) * view.scale >= MIN_MOVE_PX || kotlin.math.abs(dy) * view.scale >= MIN_MOVE_PX) {
            scene.beginGesture()
            try {
                scene.commit(scene.moveItems(selection.ids, dx, dy))
            } finally {
                scene.endGesture()
            }
            selection.box.offset(dx, dy)
        }
        dx = 0f
        dy = 0f
        show()
    }

    private fun show() {
        val framed = dragging || selection.count > 0
        val paint = if (framed) SelectionFrame(selection.box, dx, dy) else LassoFrame(loop, loopCount)
        env.show(frameId, paint)
    }

    private fun boundsOf(ids: ByteArray): RectF {
        val union = Bounds()
        for (key in ItemKey.list(ids)) {
            val render = ItemRender.parse(env.scene.itemFloats(key.bytes)) { env.scene.itemText(key.bytes) }
            render?.let { union.add(it.bounds) }
        }
        return RectF(union.left, union.top, union.right, union.bottom)
    }

    private fun addPoints(
        event: MotionEvent,
        index: Int,
    ) {
        val needed = (loopCount + event.historySize + 1) * 2
        if (loop.size < needed) loop = loop.copyOf(needed)
        for (h in 0 until event.historySize) {
            put(view.toPageX(event.getHistoricalX(index, h)), view.toPageY(event.getHistoricalY(index, h)))
        }
        put(view.toPageX(event.getX(index)), view.toPageY(event.getY(index)))
    }

    private fun put(
        x: Float,
        y: Float,
    ) {
        loop[loopCount * 2] = x
        loop[loopCount * 2 + 1] = y
        loopCount++
    }

    private companion object {
        const val INITIAL_POINTS = 64
        const val MIN_LOOP_POINTS = 3
        const val MIN_MOVE_PX = 1f
    }
}

/** Screen px a pen-down may miss the selection frame by and still grab it (a thin stroke at 0.5× is ~1 px wide). */
internal const val GRAB_SLOP_PX = 20f

/** True when page point ([x], [y]) is inside the frame inflated by [GRAB_SLOP_PX] screen px (at [scale]). */
internal fun hitsSelection(
    left: Float,
    top: Float,
    right: Float,
    bottom: Float,
    x: Float,
    y: Float,
    scale: Float,
): Boolean {
    val slop = GRAB_SLOP_PX / scale.coerceAtLeast(MIN_SCALE)
    return x >= left - slop && x <= right + slop && y >= top - slop && y <= bottom + slop
}

private const val MIN_SCALE = 1e-3f
