package dev.penwise.ink

import kotlin.math.max
import kotlin.math.min

/**
 * The page-space areas a front-buffer frame must clear and redraw to replace the previous one: a few boxes, so a
 * change confined to two edges of a big shape does not clear the shape's whole box. Pure floats, no Android types.
 */
internal class DirtyRects {
    private val values = FloatArray(CAPACITY * STRIDE)

    var size = 0
        private set

    fun clear() {
        size = 0
    }

    /** Adds the box; an empty one is ignored. */
    fun add(
        left: Float,
        top: Float,
        right: Float,
        bottom: Float,
    ) {
        if (left >= right || top >= bottom) return
        check(size < CAPACITY) { "too many dirty rects" }
        val at = size * STRIDE
        values[at + LEFT] = left
        values[at + TOP] = top
        values[at + RIGHT] = right
        values[at + BOTTOM] = bottom
        size++
    }

    fun left(i: Int) = values[i * STRIDE + LEFT]

    fun top(i: Int) = values[i * STRIDE + TOP]

    fun right(i: Int) = values[i * STRIDE + RIGHT]

    fun bottom(i: Int) = values[i * STRIDE + BOTTOM]

    companion object {
        const val CAPACITY = 2
        private const val STRIDE = 4
        private const val LEFT = 0
        private const val TOP = 1
        private const val RIGHT = 2
        private const val BOTTOM = 3
    }
}

/** Which part of a dragged rectangle outline changed between two frames of the same drag. */
internal object RectGuideChange {
    /**
     * Rectangle drag from the fixed corner ([x0], [y0]) whose other corner moved from ([oldX], [oldY]) to
     * ([newX], [newY]): adds one box around the old and new positions of each edge that moved ([pad] covers the
     * line width), so the untouched edges stay on the buffer.
     */
    fun between(
        x0: Float,
        y0: Float,
        oldX: Float,
        oldY: Float,
        newX: Float,
        newY: Float,
        pad: Float,
        out: DirtyRects,
    ) {
        val yLow = min(y0, min(oldY, newY)) - pad
        val yHigh = max(y0, max(oldY, newY)) + pad
        val xLow = min(x0, min(oldX, newX)) - pad
        val xHigh = max(x0, max(oldX, newX)) + pad
        if (oldX != newX) out.add(min(oldX, newX) - pad, yLow, max(oldX, newX) + pad, yHigh)
        if (oldY != newY) out.add(xLow, min(oldY, newY) - pad, xHigh, max(oldY, newY) + pad)
    }
}
