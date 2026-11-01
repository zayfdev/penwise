package dev.penwise.overview

import dev.penwise.frames.LaneKey
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** Card titles, the deck summary, which card carries the check, and what a tap on each does. */
class ColumnCardsTest {
    private val lane = LaneKey("p", "/", "desktop")

    @Test
    fun a_past_column_titles_its_cards_by_round_and_variant() {
        val content = ColumnCards.of(OverviewColumn.Past(generation(2, "A" to false, "bolder hero" to true)), lane)
        assertEquals("Round 2 · OG", content.head?.title)
        assertEquals(listOf("A", "B · bolder hero"), content.variants.map { it.title })
    }

    @Test
    fun the_picked_variant_has_the_check_and_the_og_never_does() {
        val content = ColumnCards.of(OverviewColumn.Past(generation(2, "A" to false, "B" to true)), lane)
        assertEquals(listOf(false, true), content.variants.map { it.checked })
        assertEquals(false, content.head?.checked)
    }

    @Test
    fun the_deck_summary_counts_the_variations_and_names_the_pick() {
        val picked = ColumnCards.of(OverviewColumn.Past(generation(2, "A" to false, "B" to true, "C" to false)), lane)
        assertEquals("3 variations · picked B", picked.summary)
        assertEquals("1 variation", ColumnCards.of(OverviewColumn.Past(generation(1, "A" to false)), lane).summary)
        assertNull(ColumnCards.of(OverviewColumn.Past(generation(1)), lane).summary)
    }

    @Test
    fun a_past_card_opens_the_viewer_on_its_own_picture() {
        val generation = generation(2, "A" to true)
        val content = ColumnCards.of(OverviewColumn.Past(generation), lane)
        assertEquals(CardAction.ViewHistory(lane, generation, "item-2-og"), content.head?.action)
        assertEquals(CardAction.ViewHistory(lane, generation, "item-2-A"), content.variants.single().action)
        assertEquals(CardImage.Asset("asset-2-A", 1440, 7574), content.variants.single().image)
    }

    @Test
    fun the_now_column_titles_the_live_frame_with_its_round_and_locks_on_tap() {
        val now =
            OverviewColumn.Now(
                frame("live", 0f),
                listOf(frame("a", 1f, label = "A", chosen = true), frame("b", 2f, label = "B")),
                3,
            )
        val content = ColumnCards.of(now, lane)
        assertEquals("Now · round 3 · OG", content.head?.title)
        assertEquals(CardImage.Still("live"), content.head?.image)
        assertEquals(CardAction.LockFrame("live"), content.head?.action)
        assertEquals(listOf(true, false), content.variants.map { it.checked })
        assertEquals("2 variations · picked A", content.summary)
        assertEquals(CardAction.LockFrame("b"), content.variants[1].action)
    }

    @Test
    fun a_sketch_column_is_a_placeholder_titled_with_the_page_that_opens_it() {
        val content = ColumnCards.of(OverviewColumn.Sketch(SKETCH_1, "Sketch 1"), lane)
        assertEquals("Sketch 1", content.head?.title)
        assertEquals(CardImage.Placeholder("Sketch 1"), content.head?.image)
        assertEquals(CardAction.OpenSketch(SKETCH_1, lane), content.head?.action)
    }

    @Test
    fun the_plus_column_has_no_cards() {
        val content = ColumnCards.of(OverviewColumn.Plus, lane)
        assertNull(content.head)
        assertTrue(content.variants.isEmpty())
    }

    @Test
    fun a_variant_named_only_by_its_letter_is_not_doubled() {
        assertEquals("A", ColumnCards.variantName(0, "a"))
        assertEquals("C", ColumnCards.variantName(2, ""))
        assertEquals("B · warm", ColumnCards.variantName(1, " warm "))
    }

    @Test
    fun a_sent_round_is_named_sent_on_the_now_column() {
        val sent = OverviewColumn.Now(frame("live", 0f), emptyList(), 6, sent = true)
        assertEquals("Now · round 6 · sent", ColumnCards.of(sent, lane).head?.title)
    }
}
