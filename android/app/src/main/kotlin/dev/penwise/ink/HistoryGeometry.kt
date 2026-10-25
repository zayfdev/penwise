package dev.penwise.ink

/** Rebuilds the geometry that persisted undo steps refer to, so undo can paint them. */
internal object HistoryGeometry {
    fun rebuild(steps: List<HistoryStep>) {
        steps.forEach { step ->
            when (step) {
                is HistoryStep.Added -> {
                    if (step.stroke.geometry == null) step.stroke.rebuild()
                }

                is HistoryStep.Removed -> {
                    step.removed.forEach { (_, stroke) ->
                        if (stroke.geometry == null) stroke.rebuild()
                    }
                }

                is HistoryStep.Replaced -> {
                    (step.removed + step.added).forEach { if (it.geometry == null) it.rebuild() }
                }
            }
        }
    }
}
