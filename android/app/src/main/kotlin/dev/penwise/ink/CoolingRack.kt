package dev.penwise.ink

import android.graphics.Bitmap
import android.graphics.BlendMode
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Matrix
import android.graphics.Paint
import androidx.core.graphics.withMatrix

/**
 * Holds bitmaps that were just replaced as the shown layer. A hardware canvas records `drawBitmap` and reads the
 * pixels when it replays, so a bitmap drawn a moment ago must not be reused (painted into, recycled) at once: after
 * [FRAMES] calls to [tick] it is passed to [ready]. Render thread only.
 */
internal class CoolingRack(
    private val ready: (Bitmap) -> Unit,
) {
    private val racked = ArrayDeque<Pair<Bitmap, Long>>()
    private var frame = 0L

    fun add(bitmap: Bitmap) {
        racked.addLast(bitmap to frame)
    }

    /** One frame passed: bitmaps that rested long enough are handed on. */
    fun tick() {
        frame++
        while (racked.isNotEmpty() && frame - racked.first().second >= FRAMES) ready(racked.removeFirst().first)
    }

    private companion object {
        /** Frames a replaced bitmap rests: its last draw has long replayed by then. */
        const val FRAMES = 3L
    }
}

/** Draws a bitmap rendered under one view as it looks under another: one `drawBitmap` through a scale + shift. */
internal class ScaledDraw {
    private val matrix = Matrix()
    private val paint = Paint(Paint.FILTER_BITMAP_FLAG)

    fun draw(
        target: Canvas,
        bitmap: Bitmap,
        from: ViewTransform,
        to: ViewTransform,
    ) {
        val k = to.scale / from.scale
        matrix.setScale(k, k)
        matrix.postTranslate(to.tx - k * from.tx, to.ty - k * from.ty)
        target.withMatrix(matrix) { drawBitmap(bitmap, 0f, 0f, paint) }
    }

    /**
     * Clears [target] to [backdrop] (transparent when null), then draws [bitmap] (if any) as [draw] does. True when a
     * bitmap was drawn, so it is now exposed to the canvas's replay.
     */
    fun drawOver(
        target: Canvas,
        bitmap: Bitmap?,
        from: ViewTransform,
        to: ViewTransform,
        backdrop: Int?,
    ): Boolean {
        if (backdrop != null) target.drawColor(backdrop) else target.drawColor(Color.TRANSPARENT, BlendMode.CLEAR)
        bitmap ?: return false
        draw(target, bitmap, from, to)
        return true
    }
}
