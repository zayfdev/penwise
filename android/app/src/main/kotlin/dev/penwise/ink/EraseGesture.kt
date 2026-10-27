package dev.penwise.ink

import android.os.SystemClock
import android.view.MotionEvent

/**
 * An eraser sweep: the whole sweep is one undo step, and the core erases it in chunks, at most one per display
 * frame, each along the points collected since the previous chunk (starting from that chunk's last point, so
 * the sweep stays connected). Precise mode cuts strokes at the sweep, otherwise whole strokes go.
 */
internal class EraseGesture(
    private val env: GestureEnv,
    event: MotionEvent,
    index: Int,
) : PenGesture {
    private val gate = PreviewGate(ERASE_FRAME_MS)
    private var points = FloatArray(INITIAL_POINTS * 2)
    private var count = 0
    private var sent = 0

    init {
        env.scene.beginGesture()
        addPoints(event, index)
        flush()
    }

    override fun move(
        event: MotionEvent,
        index: Int,
    ) {
        addPoints(event, index)
        if (gate.allow(SystemClock.uptimeMillis())) flush()
    }

    override fun end(
        event: MotionEvent,
        index: Int,
    ) {
        addPoints(event, index)
        finish()
    }

    /** Cuts already made stay (they were committed per chunk); they close as the same single undo step. */
    override fun cancel() = finish()

    private fun finish() {
        flush()
        env.scene.endGesture()
    }

    private fun addPoints(
        event: MotionEvent,
        index: Int,
    ) {
        val needed = (count + event.historySize + 1) * 2
        if (points.size < needed) points = points.copyOf(needed)
        for (h in 0 until event.historySize) put(event.getHistoricalX(index, h), event.getHistoricalY(index, h))
        put(event.getX(index), event.getY(index))
    }

    private fun put(
        x: Float,
        y: Float,
    ) {
        points[count * 2] = x
        points[count * 2 + 1] = y
        count++
    }

    private fun flush() {
        if (count == sent) return
        val from = maxOf(sent - 1, 0)
        val path = points.copyOfRange(from * 2, count * 2)
        sent = count
        val scene = env.scene
        scene.commit(scene.erase(path, env.brushes.eraserRadiusPx, env.brushes.preciseErase, Layers.ERASABLE))
    }

    private companion object {
        /** One display frame at 60 Hz. */
        const val ERASE_FRAME_MS = 16L
        const val INITIAL_POINTS = 64
    }
}
