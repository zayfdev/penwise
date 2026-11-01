package dev.penwise.overview

import dev.penwise.frames.LaneKey

/** Which decks are expanded in place (a tap toggles one). Immutable; the key names a strip's column. */
internal data class DeckState(
    private val expanded: Set<String> = emptySet(),
) {
    fun isExpanded(
        lane: LaneKey,
        column: OverviewColumn,
    ) = key(lane, column) in expanded

    fun toggled(
        lane: LaneKey,
        column: OverviewColumn,
    ): DeckState {
        val key = key(lane, column)
        return DeckState(if (key in expanded) expanded - key else expanded + key)
    }

    private fun key(
        lane: LaneKey,
        column: OverviewColumn,
    ) = "${lane.project}|${lane.route}|${lane.preset}|${column.key}"
}

/** What a tap on a strip's `+` offers. */
internal sealed interface PlusAction {
    /** An Infinite page in the board page's notebook, attached to the strip. */
    data class NewSketchPage(
        val lane: LaneKey,
    ) : PlusAction
}

/** The `+` menu and the names of new sketch pages. */
internal object PlusMenu {
    fun entries(lane: LaneKey): List<PlusAction> = listOf(PlusAction.NewSketchPage(lane))

    fun label(action: PlusAction): String =
        when (action) {
            is PlusAction.NewSketchPage -> "New sketch page"
        }

    /** "Sketch n": the lowest n (from 1) no page of the notebook already carries. */
    fun nextSketchTitle(existing: Collection<String>): String {
        val taken = existing.map { it.trim() }.toSet()
        return generateSequence(1) { it + 1 }.map { "Sketch $it" }.first { it !in taken }
    }
}
