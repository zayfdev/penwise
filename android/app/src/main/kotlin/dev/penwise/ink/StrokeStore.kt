package dev.penwise.ink

import dev.penwise.core.InkGeometry

/**
 * Finished strokes of the page, in drawing order, with the data needed to save them.
 *
 * Mutated on the UI thread only; [strokes] is an immutable snapshot that the render thread may read at any time.
 * [onPageChanged] fires after undo/clear/erase so the surface can repaint; [add] does not fire it because the caller
 * commits the front buffer, which repaints anyway. [onAppend] and [onRewrite] are the persistence hooks.
 */
class StrokeStore {
    private val entries = ArrayList<SavedStroke>()

    @Volatile
    var strokes: List<InkGeometry> = emptyList()
        private set

    var onPageChanged: (() -> Unit)? = null

    var onAppend: ((SavedStroke) -> Unit)? = null

    var onRewrite: ((List<SavedStroke>) -> Unit)? = null

    /** Strokes loaded from disk, with geometry already rebuilt. */
    fun restore(saved: List<SavedStroke>) {
        entries.clear()
        saved.mapNotNullTo(entries) { stroke -> if (stroke.geometry != null) stroke else null }
        publish()
    }

    fun add(stroke: SavedStroke) {
        entries.add(stroke)
        publish()
        onAppend?.invoke(stroke)
    }

    fun undo() {
        if (entries.isEmpty()) return
        entries.removeAt(entries.lastIndex)
        publish()
        onPageChanged?.invoke()
        persist()
    }

    fun clear() {
        if (entries.isEmpty()) return
        entries.clear()
        publish()
        onPageChanged?.invoke()
        persist()
    }

    /**
     * Removes every stroke whose centreline passes within [radius] of the drag points (x, y interleaved).
     * Returns true when something was removed. Pass `persist = false` while the finger is still moving.
     */
    fun eraseUnder(
        points: FloatArray,
        count: Int,
        radius: Float,
        persist: Boolean = true,
    ): Boolean {
        if (count == 0) return false
        val before = entries.size
        entries.removeAll { stroke -> EraseHit.hits(stroke, points, count, radius) }
        if (entries.size == before) return false
        publish()
        onPageChanged?.invoke()
        if (persist) persist()
        return true
    }

    /** A copy of the stored strokes, for resending the page. */
    fun snapshot(): List<SavedStroke> = entries.toList()

    /** Writes the current stroke list back to disk. */
    fun persist() {
        onRewrite?.invoke(entries.toList())
    }

    private fun publish() {
        strokes = entries.mapNotNull { it.geometry }
    }
}
