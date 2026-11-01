package dev.penwise.frames

/** One entry of the variant switcher. */
internal data class SwitcherItem(
    val id: String,
    /** "OG" for the lane's live frame, a variant's label otherwise. */
    val label: String,
    val og: Boolean,
    /** The owner picked this variant. */
    val chosen: Boolean,
)

/**
 * What the variant switcher shows (locked-frames contract §3.2): `OG`, then the lane's variants by label, with the
 * locked frame highlighted. Pure, so the order and the selection are unit-tested without Compose.
 */
internal class SwitcherState(
    val items: List<SwitcherItem>,
    /** The locked frame's id; it may be no item (a plain frame that is not the lane's live one). */
    val shown: String,
) {
    private val shownItem: SwitcherItem? get() = items.firstOrNull { it.id == shown }

    /** "Use this" picks the shown variant: not on OG, and not when it already is the pick. */
    val canUseThis: Boolean get() = shownItem?.let { !it.og && !it.chosen } ?: false

    /** The shown item has a neighbour on some side: an edge swipe has somewhere to go. */
    val hasNeighbour: Boolean get() = neighbour(-1) != null || neighbour(1) != null

    /** The item [delta] places after (or before) the shown one, or null past the row's end: an edge swipe's target. */
    fun neighbour(delta: Int): String? {
        val index = items.indexOfFirst { it.id == shown }
        if (index < 0) return null
        return items.getOrNull(index + delta)?.id
    }

    companion object {
        const val OG_LABEL = "OG"

        /** The switcher for the lane of the [locked] frame; null when it has no variants (or nothing is locked). */
        fun of(
            specs: List<FrameSpec>,
            locked: String?,
        ): SwitcherState? {
            val spec = specs.firstOrNull { it.id == locked } ?: return null
            val variants = FrameLane.variants(specs, spec.lane)
            if (variants.isEmpty()) return null
            val live = FrameLane.live(specs, spec.lane)
            val items =
                listOfNotNull(live?.let { SwitcherItem(it.id, OG_LABEL, og = true, chosen = false) }) +
                    variants.map { SwitcherItem(it.id, it.label.ifEmpty { "?" }, og = false, chosen = it.chosen) }
            return SwitcherState(items, spec.id)
        }
    }
}
