package dev.penwise.overview

import dev.penwise.frames.LaneKey
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** The strips: one per screen, columns oldest to newest, then sketch pages, then `+`. */
class OverviewStripsTest {
    private val desktop = LaneKey("p", "/", "desktop")
    private val phone = LaneKey("p", "/", "phone")

    private val specs =
        listOf(
            frame("phone-live", 0f, y = 2000f, preset = "phone"),
            frame("d-old", 0f),
            frame("d-live", 3000f),
            frame("d-a", 1500f, label = "A", group = "g"),
            frame("d-b", 2200f, label = "B", group = "g", chosen = true),
        )

    private fun strips(
        history: Map<LaneKey, List<Generation>> = emptyMap(),
        pages: Map<LaneKey, List<StripPage>> = emptyMap(),
        titles: Map<String, String> = emptyMap(),
        rounds: Map<String, NowRound> = emptyMap(),
    ) = OverviewStrips.of(specs, history, pages, rounds) { titles[it] }

    @Test
    fun there_is_one_strip_per_screen_with_the_top_lane_first() {
        val result = strips()
        assertEquals(listOf(desktop, phone), result.map { it.lane })
        assertEquals(listOf("Desktop /", "Phone /"), result.map { it.title })
        assertEquals("d-live", result[0].live.id)
    }

    @Test
    fun a_strip_without_history_is_now_then_plus() {
        val columns = strips()[0].columns
        assertEquals(listOf("now", "plus"), columns.map { it.key })
        assertEquals(0, strips()[0].nowIndex)
    }

    @Test
    fun past_generations_come_oldest_first_before_the_now_column() {
        val history = mapOf(desktop to listOf(generation(3), generation(2), generation(1)))
        val strip = strips(history)[0]
        assertEquals(listOf("past-1", "past-2", "past-3", "now", "plus"), strip.columns.map { it.key })
        assertEquals(3, strip.nowIndex)
    }

    @Test
    fun history_of_another_screen_stays_on_its_strip() {
        val history = mapOf(phone to listOf(generation(1)))
        assertEquals(listOf("now", "plus"), strips(history)[0].columns.map { it.key })
        assertEquals(listOf("past-1", "now", "plus"), strips(history)[1].columns.map { it.key })
    }

    @Test
    fun the_now_column_holds_the_live_frame_and_its_variants_by_label() {
        val now = strips()[0].columns.filterIsInstance<OverviewColumn.Now>().single()
        assertEquals("d-live", now.og.id)
        assertEquals(listOf("d-a", "d-b"), now.variants.map { it.id })
    }

    @Test
    fun sketch_pages_follow_now_in_attach_order_and_a_deleted_page_drops_out() {
        val pages =
            mapOf(
                desktop to
                    listOf(
                        StripPage("s1", "/", "desktop"),
                        StripPage("gone", "/", "desktop"),
                        StripPage("s2", "/", "desktop"),
                    ),
            )
        val columns = strips(pages = pages, titles = mapOf("s1" to "Sketch 1", "s2" to "Sketch 2"))[0].columns
        assertEquals(listOf("now", "sketch-s1", "sketch-s2", "plus"), columns.map { it.key })
        assertEquals("Sketch 2", (columns[2] as OverviewColumn.Sketch).title)
    }

    @Test
    fun a_lane_that_has_only_variants_has_no_strip() {
        val only = listOf(frame("v", 100f, label = "A", group = "g"))
        assertTrue(OverviewStrips.of(only, emptyMap(), emptyMap()) { null }.isEmpty())
    }

    @Test
    fun the_strip_that_opens_is_the_one_of_the_lane_asked_for() {
        val result = strips()
        assertEquals(1, OverviewStrips.indexOf(result, phone))
        assertEquals(0, OverviewStrips.indexOf(result, desktop))
        assertEquals(0, OverviewStrips.indexOf(result, null))
        assertEquals(0, OverviewStrips.indexOf(result, LaneKey("p", "/gone", "desktop")))
    }

    @Test
    fun the_generation_key_changes_when_a_round_moves_the_frames() {
        val moved = specs.map { if (it.id == "d-live") it.copy(x = 6000f) else it }
        val regrouped = specs.map { if (it.id == "d-a") it.copy(group = "h") else it }
        val key = OverviewStrips.generationKey(specs)
        assertEquals(key, OverviewStrips.generationKey(specs.toList()))
        assertNotEquals(key, OverviewStrips.generationKey(moved))
        assertNotEquals(key, OverviewStrips.generationKey(regrouped))
    }
}
