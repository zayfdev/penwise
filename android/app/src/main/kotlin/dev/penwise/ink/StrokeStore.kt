package dev.penwise.ink

import dev.penwise.core.InkGeometry

/**
 * Finished strokes of the page, in drawing order.
 *
 * Mutated on the UI thread only; [strokes] is an immutable snapshot that the render thread may read at any time.
 * [onPageChanged] fires after undo/clear so the surface can repaint; [add] does not fire it because the caller
 * commits the front buffer, which repaints anyway.
 */
class StrokeStore {
    @Volatile
    var strokes: List<InkGeometry> = emptyList()
        private set

    var onPageChanged: (() -> Unit)? = null

    fun add(geometry: InkGeometry) {
        strokes = strokes + geometry
    }

    fun undo() {
        if (strokes.isNotEmpty()) {
            strokes = strokes.dropLast(1)
            onPageChanged?.invoke()
        }
    }

    fun clear() {
        if (strokes.isNotEmpty()) {
            strokes = emptyList()
            onPageChanged?.invoke()
        }
    }
}
