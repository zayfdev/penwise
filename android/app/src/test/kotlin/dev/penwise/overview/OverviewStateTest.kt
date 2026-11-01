package dev.penwise.overview

import dev.penwise.frames.LaneKey
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** Deck expand/collapse, the `+` menu, the names of new sketch pages and the snap maths. */
class OverviewStateTest {
    private val lane = LaneKey("p", "/", "desktop")
    private val other = LaneKey("p", "/", "phone")
    private val column = OverviewColumn.Past(generation(2, "A" to true))

    @Test
    fun a_deck_starts_collapsed_and_a_tap_toggles_it() {
        val closed = DeckState()
        assertFalse(closed.isExpanded(lane, column))
        val open = closed.toggled(lane, column)
        assertTrue(open.isExpanded(lane, column))
        assertFalse(open.toggled(lane, column).isExpanded(lane, column))
    }

    @Test
    fun decks_of_other_strips_and_columns_are_separate() {
        val open = DeckState().toggled(lane, column)
        assertFalse(open.isExpanded(other, column))
        assertFalse(open.isExpanded(lane, OverviewColumn.Past(generation(3, "A" to true))))
    }

    @Test
    fun the_plus_menu_offers_a_new_sketch_page_for_its_strip() {
        assertEquals(listOf(PlusAction.NewSketchPage(lane)), PlusMenu.entries(lane))
        assertEquals("New sketch page", PlusMenu.label(PlusMenu.entries(lane).single()))
    }

    @Test
    fun a_new_sketch_page_takes_the_lowest_free_number() {
        assertEquals("Sketch 1", PlusMenu.nextSketchTitle(listOf("Page 1")))
        assertEquals("Sketch 3", PlusMenu.nextSketchTitle(listOf("Sketch 1", "Sketch 2", "Home")))
        assertEquals("Sketch 2", PlusMenu.nextSketchTitle(listOf("Sketch 1", "Sketch 3")))
        assertEquals("Sketch 1", PlusMenu.nextSketchTitle(listOf(" Sketch 2 ")))
    }

    @Test
    fun cards_are_about_half_the_screen_tall_and_a_neighbour_always_peeks() {
        val desktop = OverviewMetrics.of(1280f, 800f, 1440f / 900f)
        assertEquals(800f * 0.46f, desktop.cardHeight, 0.01f)
        assertTrue(desktop.peek > 0f)
        assertEquals(1280f, desktop.cardWidth + 2 * desktop.sidePadding, 0.01f)
    }

    @Test
    fun a_wide_frame_is_capped_so_it_never_fills_the_screen() {
        val wide = OverviewMetrics.of(1280f, 800f, 4f)
        assertEquals(1280f * 0.58f, wide.cardWidth, 0.01f)
        assertEquals(wide.cardWidth / 4f, wide.cardHeight, 0.01f)
        assertTrue(wide.peek > 0f)
    }

    @Test
    fun a_phone_frame_gets_a_narrow_card() {
        val phone = OverviewMetrics.of(1280f, 800f, 390f / 844f)
        assertTrue(phone.cardWidth < 250f)
        assertEquals(phone.cardWidth / (390f / 844f), phone.cardHeight, 0.01f)
    }

    @Test
    fun the_strip_stays_on_its_column_when_columns_arrive_on_the_left() {
        val before = listOf("now", "plus")
        val after = listOf("past-1", "past-2", "now", "plus")
        assertEquals(0, SnapIndex.after("now", before, 0))
        assertEquals(2, SnapIndex.after("now", after, 0))
        assertEquals(3, SnapIndex.after("plus", after, 1))
    }

    @Test
    fun a_column_that_is_gone_falls_back_inside_the_list() {
        val keys = listOf("now", "plus")
        assertEquals(1, SnapIndex.after("sketch-x", keys, 5))
        assertEquals(0, SnapIndex.after(null, keys, -2))
        assertEquals(0, SnapIndex.after("x", emptyList(), 3))
    }

    @Test
    fun the_motions_stay_inside_the_contracts_limits() {
        assertTrue(OverviewMotion.ZOOM_MS <= 260)
        assertTrue(OverviewMotion.DECK_MS <= 220)
        assertEquals(OverviewMotion.Motion.CROSSFADE, OverviewMotion.zoom(reduced = true))
        assertEquals(OverviewMotion.Motion.ZOOM, OverviewMotion.zoom(reduced = false))
    }
}
