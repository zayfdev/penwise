package dev.penwise.ink

import dev.penwise.core.InkGeometry

/**
 * Finished strokes of the page, in drawing order, with the data needed to save them.
 *
 * Mutated on the UI thread only; [strokes] is an immutable snapshot that the render thread may read at any time.
 * [onPageChanged] fires after undo/redo so the surface can repaint; [add] does not fire it because the caller commits
 * the front buffer, which repaints anyway. [onAppend] and [onRewrite] are the persistence hooks.
 *
 * Undo covers every change, including erases and clear: one gesture is one step, and redo replays it.
 */
class StrokeStore {
    private val entries = ArrayList<SavedStroke>()

    /** One undoable step: strokes appended, or strokes removed with their original positions. */
    private sealed interface Step {
        data class Added(
            val stroke: SavedStroke,
        ) : Step

        data class Removed(
            val removed: List<Pair<Int, SavedStroke>>,
        ) : Step

        /** A precise erase: strokes swapped for the fragments they left behind. */
        data class Replaced(
            val removed: List<SavedStroke>,
            val added: List<SavedStroke>,
        ) : Step
    }

    private val undoStack = ArrayDeque<Step>()
    private val redoStack = ArrayDeque<Step>()

    @Volatile
    var strokes: List<InkGeometry> = emptyList()
        private set

    var onPageChanged: (() -> Unit)? = null

    var onAppend: ((SavedStroke) -> Unit)? = null

    var onRewrite: ((List<SavedStroke>) -> Unit)? = null

    /** Fires for edits that the mirror can apply incrementally instead of resending the page. */
    var onOp: ((PageOp) -> Unit)? = null

    /** Strokes loaded from disk, with geometry already rebuilt. Undo history is not restored. */
    fun restore(saved: List<SavedStroke>) {
        entries.clear()
        saved.mapNotNullTo(entries) { stroke -> if (stroke.geometry != null) stroke else null }
        undoStack.clear()
        redoStack.clear()
        publish()
    }

    fun add(stroke: SavedStroke) {
        entries.add(stroke)
        record(Step.Added(stroke))
        redoStack.clear()
        publish()
        onAppend?.invoke(stroke)
    }

    /**
     * Adds strokes this device does not have yet (agent marks, other devices), matched by seed.
     * Returns true when something was added. Geometry must already be built.
     */
    fun merge(incoming: List<SavedStroke>): Boolean {
        val known = entries.mapTo(HashSet()) { it.seed }
        val added = incoming.filter { it.seed !in known && it.geometry != null }
        if (added.isEmpty()) return false
        entries.addAll(added)
        publish()
        onPageChanged?.invoke()
        return true
    }

    fun canUndo(): Boolean = undoStack.isNotEmpty()

    fun canRedo(): Boolean = redoStack.isNotEmpty()

    fun undo() {
        val step = undoStack.removeLastOrNull() ?: return
        when (step) {
            is Step.Added -> {
                entries.removeAll { it === step.stroke }
                onOp?.invoke(PageOp.Remove(listOf(step.stroke.seed)))
            }

            is Step.Removed -> {
                // Reinsert at the original indices, ascending, so positions land where they were.
                step.removed.sortedBy { it.first }.forEach { (index, stroke) ->
                    entries.add(index.coerceAtMost(entries.size), stroke)
                }
                onOp?.invoke(PageOp.Add(step.removed.map { it.second }))
            }

            is Step.Replaced -> {
                entries.removeAll { entry -> step.added.any { it.seed == entry.seed } }
                entries.addAll(step.removed)
                onOp?.invoke(PageOp.Replace(step.added.map { it.seed }, step.removed))
            }
        }
        redoStack.addLast(step)
        publish()
        onPageChanged?.invoke()
        persist()
    }

    fun redo() {
        val step = redoStack.removeLastOrNull() ?: return
        when (step) {
            is Step.Added -> {
                entries.add(step.stroke)
                onOp?.invoke(PageOp.Add(listOf(step.stroke)))
            }

            is Step.Removed -> {
                entries.removeAll { entry -> step.removed.any { it.second === entry } }
                onOp?.invoke(PageOp.Remove(step.removed.map { it.second.seed }))
            }

            is Step.Replaced -> {
                entries.removeAll { entry -> step.removed.any { it.seed == entry.seed } }
                entries.addAll(step.added)
                onOp?.invoke(PageOp.Replace(step.removed.map { it.seed }, step.added))
            }
        }
        undoStack.addLast(step)
        publish()
        onPageChanged?.invoke()
        persist()
    }

    fun clear() {
        if (entries.isEmpty()) return
        val removed = entries.mapIndexed { index, stroke -> index to stroke }
        entries.clear()
        record(Step.Removed(removed))
        redoStack.clear()
        publish()
        onPageChanged?.invoke()
        onOp?.invoke(PageOp.Clear)
        persist()
    }

    /**
     * Removes every stroke whose centreline passes within [radius] of the drag points (x, y interleaved).
     * Returns true when something was removed. While the pen is still moving, pass `persist = false` and
     * `record = false`, then close the gesture once with [commitErase].
     */
    fun eraseUnder(
        points: FloatArray,
        count: Int,
        radius: Float,
        persist: Boolean = true,
    ): Boolean {
        if (count == 0) return false
        val removed = ArrayList<Pair<Int, SavedStroke>>()
        var index = 0
        while (index < entries.size) {
            val stroke = entries[index]
            if (EraseHit.hits(stroke, points, count, radius)) {
                removed += index to stroke
                entries.removeAt(index)
            } else {
                index++
            }
        }
        if (removed.isEmpty()) return false
        mergeErase(removed)
        publish()
        onPageChanged?.invoke()
        onOp?.invoke(PageOp.Remove(removed.map { it.second.seed }))
        if (persist) persist()
        return true
    }

    /** Ends an erase gesture without another hit, so one drag is exactly one undo step. */
    fun commitErase() {
        redoStack.clear()
        persist()
    }

    /**
     * Replaces strokes by seed with the fragments a precise erase cut out of them; one undo step for the whole
     * swap. Fragments keep the page order at the end of the list, which matches how they are painted (on top).
     */
    fun replace(
        seeds: List<Long>,
        fragments: List<SavedStroke>,
    ): Boolean {
        val removed =
            entries.mapIndexedNotNull { index, entry ->
                index.takeIf { entry.seed in seeds }?.let {
                    it to
                        entry
                }
            }
        if (removed.isEmpty()) return false
        entries.removeAll { entry -> entry.seed in seeds }
        entries.addAll(fragments)
        record(Step.Replaced(removed.map { it.second }, fragments))
        redoStack.clear()
        publish()
        onPageChanged?.invoke()
        onOp?.invoke(PageOp.Replace(seeds, fragments))
        persist()
        return true
    }

    /** A copy of the stored strokes, for resending the page. */
    fun snapshot(): List<SavedStroke> = entries.toList()

    /** Writes the current stroke list back to disk. */
    fun persist() {
        onRewrite?.invoke(entries.toList())
    }

    private fun record(step: Step) {
        undoStack.addLast(step)
        if (undoStack.size > MAX_STEPS) undoStack.removeFirst()
    }

    /** Folds an erase call into the open step, so a drag that erases repeatedly undoes as one action. */
    private fun mergeErase(removed: List<Pair<Int, SavedStroke>>) {
        val last = undoStack.lastOrNull()
        if (last is Step.Removed) {
            undoStack.removeLast()
            undoStack.addLast(Step.Removed(last.removed + removed))
        } else {
            record(Step.Removed(removed))
        }
    }

    private fun publish() {
        strokes = entries.mapNotNull { it.geometry }
    }

    private companion object {
        const val MAX_STEPS = 200
    }
}
