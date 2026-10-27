package dev.penwise.ink

import dev.penwise.core.InkGeometry

/** An edit that arrived from elsewhere; applied without history. */
sealed interface RemoteOp {
    data class Remove(
        val seeds: Set<Long>,
    ) : RemoteOp

    data object Clear : RemoteOp

    data class Replace(
        val seeds: Set<Long>,
        val fragments: List<SavedStroke>,
    ) : RemoteOp

    data class Add(
        val strokes: List<SavedStroke>,
    ) : RemoteOp
}

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

    private val undoStack = ArrayDeque<HistoryStep>()
    private val redoStack = ArrayDeque<HistoryStep>()

    @Volatile
    var strokes: List<InkGeometry> = emptyList()
        private set

    var onPageChanged: (() -> Unit)? = null

    var onAppend: ((SavedStroke) -> Unit)? = null

    var onRewrite: ((List<SavedStroke>) -> Unit)? = null

    /** Fires for edits that the mirror can apply incrementally instead of resending the page. */
    var onOp: ((PageOp) -> Unit)? = null

    /** Fires whenever the undo or redo stack changes, so the history can be persisted. */
    var onHistoryChanged: (() -> Unit)? = null

    /** Strokes loaded from disk, with geometry already rebuilt. Undo history is cleared. */
    fun restore(saved: List<SavedStroke>) {
        entries.clear()
        saved.mapNotNullTo(entries) { stroke -> if (stroke.geometry != null) stroke else null }
        undoStack.clear()
        redoStack.clear()
        publish()
        onHistoryChanged?.invoke()
    }

    /** The current stacks, oldest first, for persisting them. */
    fun historySnapshot(): Pair<List<HistoryStep>, List<HistoryStep>> = undoStack.toList() to redoStack.toList()

    /** Puts persisted steps back after [restore]; geometry is rebuilt by the caller. */
    fun restoreHistory(
        undo: List<HistoryStep>,
        redo: List<HistoryStep>,
    ) {
        undoStack.clear()
        undoStack.addAll(undo.takeLast(MAX_STEPS))
        redoStack.clear()
        redoStack.addAll(redo.takeLast(MAX_STEPS))
        onHistoryChanged?.invoke()
    }

    fun add(stroke: SavedStroke) {
        entries.add(stroke)
        record(HistoryStep.Added(stroke))
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

    /**
     * Applies an edit made elsewhere (another client, or the server restoring an archive).
     *
     * Remote edits are not ours to undo: they never enter the history and never echo back as ops, or a
     * two-client page would ping-pong the same removal forever.
     */
    fun applyRemote(op: RemoteOp): Boolean {
        val changed =
            when (op) {
                is RemoteOp.Remove -> {
                    entries.removeAll { it.seed in op.seeds }
                }

                is RemoteOp.Clear -> {
                    entries.isNotEmpty().also { entries.clear() }
                }

                is RemoteOp.Replace -> {
                    val removed = entries.removeAll { it.seed in op.seeds }
                    addUnknown(op.fragments) or removed
                }

                is RemoteOp.Add -> {
                    addUnknown(op.strokes)
                }
            }
        if (!changed) return false
        publish()
        onPageChanged?.invoke()
        persist()
        return true
    }

    /** Appends the strokes whose seed is not on the page yet; the server echoes this device's own ops back. */
    private fun addUnknown(incoming: List<SavedStroke>): Boolean {
        val known = entries.mapTo(HashSet()) { it.seed }
        return entries.addAll(incoming.filter { known.add(it.seed) })
    }

    /**
     * Gives every stroke that has no seed a fresh one and returns the strokes that changed.
     *
     * Strokes imported from the oldest page files had no seed at all, so several shared seed `0`. Identity is the
     * seed, which made those strokes mutually indistinguishable: one removal deleted all of them, and clients
     * collapsed them into one. Seeds are only ever compared for identity, so rewriting them is safe.
     */
    fun assignMissingSeeds(): List<SavedStroke> {
        val changed = entries.filter { it.seed == 0L }
        if (changed.isEmpty()) return emptyList()
        var next = System.currentTimeMillis()
        for (stroke in changed) {
            stroke.reseed(next++)
        }
        publish()
        onPageChanged?.invoke()
        onRewrite?.invoke(snapshot())
        return changed
    }

    fun canUndo(): Boolean = undoStack.isNotEmpty()

    fun canRedo(): Boolean = redoStack.isNotEmpty()

    fun undo() {
        val step = undoStack.removeLastOrNull() ?: return
        when (step) {
            is HistoryStep.Added -> {
                // Seeds are the stroke's identity (they survive a restart); instances are not.
                entries.removeAll { it.seed == step.stroke.seed }
                onOp?.invoke(PageOp.Remove(listOf(step.stroke.seed)))
            }

            is HistoryStep.Removed -> {
                // Reinsert at the original indices, ascending, so positions land where they were.
                step.removed.sortedBy { it.first }.forEach { (index, stroke) ->
                    entries.add(index.coerceAtMost(entries.size), stroke)
                }
                onOp?.invoke(PageOp.Add(step.removed.map { it.second }))
            }

            is HistoryStep.Replaced -> {
                entries.removeAll { entry -> step.added.any { it.seed == entry.seed } }
                entries.addAll(step.removed)
                onOp?.invoke(PageOp.Replace(step.added.map { it.seed }, step.removed))
            }
        }
        redoStack.addLast(step)
        publish()
        onPageChanged?.invoke()
        onHistoryChanged?.invoke()
        persist()
    }

    fun redo() {
        val step = redoStack.removeLastOrNull() ?: return
        when (step) {
            is HistoryStep.Added -> {
                entries.add(step.stroke)
                onOp?.invoke(PageOp.Add(listOf(step.stroke)))
            }

            is HistoryStep.Removed -> {
                entries.removeAll { entry -> step.removed.any { it.second.seed == entry.seed } }
                onOp?.invoke(PageOp.Remove(step.removed.map { it.second.seed }))
            }

            is HistoryStep.Replaced -> {
                entries.removeAll { entry -> step.removed.any { it.seed == entry.seed } }
                entries.addAll(step.added)
                onOp?.invoke(PageOp.Replace(step.removed.map { it.seed }, step.added))
            }
        }
        undoStack.addLast(step)
        publish()
        onPageChanged?.invoke()
        onHistoryChanged?.invoke()
        persist()
    }

    fun clear() {
        if (entries.isEmpty()) return
        val removed = entries.mapIndexed { index, stroke -> index to stroke }
        entries.clear()
        record(HistoryStep.Removed(removed))
        redoStack.clear()
        publish()
        onPageChanged?.invoke()
        onOp?.invoke(PageOp.Clear)
        onHistoryChanged?.invoke()
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
        onHistoryChanged?.invoke()
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
        record(HistoryStep.Replaced(removed.map { it.second }, fragments))
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

    private fun record(step: HistoryStep) {
        undoStack.addLast(step)
        if (undoStack.size > MAX_STEPS) undoStack.removeFirst()
        onHistoryChanged?.invoke()
    }

    /** Folds an erase call into the open step, so a drag that erases repeatedly undoes as one action. */
    private fun mergeErase(removed: List<Pair<Int, SavedStroke>>) {
        val last = undoStack.lastOrNull()
        if (last is HistoryStep.Removed) {
            undoStack.removeLast()
            undoStack.addLast(HistoryStep.Removed(last.removed + removed))
        } else {
            record(HistoryStep.Removed(removed))
        }
    }

    private fun publish() {
        strokes = entries.mapNotNull { it.geometry }
    }

    private companion object {
        const val MAX_STEPS = 200
    }
}
