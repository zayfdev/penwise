package dev.penwise.ink

/** Rebuilds the geometry that persisted undo steps refer to, so undo can paint them. */
internal object HistoryGeometry {
    /** The steps whose strokes could all be rebuilt; a step the engine refuses is dropped, not fatal. */
    fun usable(
        steps: List<HistoryStep>,
        build: (SavedStroke) -> Unit = { it.rebuild() },
        onSkip: SkipReport = StrokeRebuild.logSkip,
    ): List<HistoryStep> = steps.filter { step -> strokesOf(step).all { StrokeRebuild.tryBuild(it, build, onSkip) } }

    private fun strokesOf(step: HistoryStep): List<SavedStroke> =
        when (step) {
            is HistoryStep.Added -> listOf(step.stroke)
            is HistoryStep.Removed -> step.removed.map { it.second }
            is HistoryStep.Replaced -> step.removed + step.added
        }
}
