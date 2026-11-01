package dev.penwise.overview

import dev.penwise.frames.FrameSpec
import dev.penwise.frames.LaneKey

/** What a card shows: a frame's still, a kept picture's asset, or (a sketch page) a light placeholder. */
internal sealed interface CardImage {
    data class Still(
        val frameId: String,
    ) : CardImage

    data class Asset(
        val asset: String,
        val width: Int,
        val height: Int,
    ) : CardImage

    data class Placeholder(
        val title: String,
    ) : CardImage
}

/** What a tap on a card does (contract §3.3, "Entering"). */
internal sealed interface CardAction {
    /** A live frame or a live variant: the board locks to it. */
    data class LockFrame(
        val frameId: String,
    ) : CardAction

    /** A past picture: the read-only viewer opens on it. */
    data class ViewHistory(
        val lane: LaneKey,
        val generation: Generation,
        val item: String,
    ) : CardAction

    /** A sketch page: opened as a free canvas. */
    data class OpenSketch(
        val page: String,
        val lane: LaneKey,
    ) : CardAction
}

/** A card with the small title above it. [checked]: the picked variant. */
internal data class OverviewCard(
    val id: String,
    val title: String,
    val image: CardImage,
    val checked: Boolean,
    val action: CardAction,
)

/** What one column shows: the head card, the variant cards of its deck, and the deck's collapsed summary. */
internal data class ColumnContent(
    val head: OverviewCard?,
    val variants: List<OverviewCard>,
    val summary: String?,
)

/** The cards, titles and the deck summary of a column. Pure. */
internal object ColumnCards {
    fun of(
        column: OverviewColumn,
        lane: LaneKey,
    ): ColumnContent =
        when (column) {
            is OverviewColumn.Past -> past(column.generation, lane)
            is OverviewColumn.Now -> now(column)
            is OverviewColumn.Sketch -> sketch(column, lane)
            OverviewColumn.Plus -> ColumnContent(null, emptyList(), null)
        }

    /** The letter of the [index]th variant: A, B, C... */
    fun letter(index: Int): String = ('A' + index).toString()

    /** "A" when the label says nothing more than the letter, else "A · label". */
    fun variantName(
        index: Int,
        label: String,
    ): String {
        val letter = letter(index)
        val text = label.trim()
        return if (text.isEmpty() || text.equals(letter, ignoreCase = true)) letter else "$letter · $text"
    }

    /** "3 variations · picked B" / "1 variation"; [pickedIndex] is the picked variant's place, or null. */
    fun summary(
        count: Int,
        pickedIndex: Int?,
    ): String? {
        if (count == 0) return null
        val noun = if (count == 1) "variation" else "variations"
        return "$count $noun" + (pickedIndex?.let { " · picked ${letter(it)}" } ?: "")
    }

    private fun past(
        generation: Generation,
        lane: LaneKey,
    ): ColumnContent {
        fun picture(item: HistoryPicture) = CardImage.Asset(item.asset, item.width, item.height)

        fun enter(item: HistoryPicture) = CardAction.ViewHistory(lane, generation, item.item)
        val head =
            OverviewCard(
                "${generation.round}-og",
                "Round ${generation.round} · OG",
                picture(generation.og),
                checked = false,
                action = enter(generation.og),
            )
        val variants =
            generation.variants.mapIndexed { index, variant ->
                OverviewCard(
                    "${generation.round}-${variant.picture.item}",
                    variantName(index, variant.label),
                    picture(variant.picture),
                    checked = variant.picked,
                    action = enter(variant.picture),
                )
            }
        return ColumnContent(
            head,
            variants,
            summary(
                variants.size,
                generation.variants.indexOfFirst { it.picked }.takeIf {
                    it >=
                        0
                },
            ),
        )
    }

    private fun now(column: OverviewColumn.Now): ColumnContent {
        fun card(
            spec: FrameSpec,
            title: String,
        ) = OverviewCard(spec.id, title, CardImage.Still(spec.id), spec.chosen, CardAction.LockFrame(spec.id))
        val head = card(column.og, "Now · round ${column.round}${if (column.sent) " · sent" else " · OG"}")
        val variants = column.variants.mapIndexed { index, spec -> card(spec, variantName(index, spec.label)) }
        return ColumnContent(
            head,
            variants,
            summary(
                variants.size,
                column.variants.indexOfFirst { it.chosen }.takeIf {
                    it >=
                        0
                },
            ),
        )
    }

    private fun sketch(
        column: OverviewColumn.Sketch,
        lane: LaneKey,
    ): ColumnContent =
        ColumnContent(
            OverviewCard(
                "sketch-${column.page}",
                column.title,
                CardImage.Placeholder(column.title),
                checked = false,
                action = CardAction.OpenSketch(column.page, lane),
            ),
            emptyList(),
            null,
        )
}
