package dev.penwise.overview

import dev.penwise.frames.LaneKey
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** The overview's state machine and the switcher row of a past generation. */
class OverviewControllerTest {
    private val specs = listOf(frame("live", 0f))
    private val lane = LaneKey("p", "/", "desktop")
    private val generation = generation(2, "A" to false, "B" to true)

    @Test
    fun a_request_on_the_board_opens_the_overview_with_its_frames() {
        val controller = OverviewController()
        controller.request("board", specs)
        val ui = controller.ui.value
        assertTrue(ui.open)
        assertEquals("board", ui.boardPage)
        assertEquals(specs, ui.specs)
    }

    @Test
    fun a_request_with_no_board_seen_opens_nothing() {
        val controller = OverviewController()
        controller.request("page", emptyList())
        assertFalse(controller.ui.value.open)
    }

    @Test
    fun a_sketch_page_reopens_the_overview_from_the_boards_copy() {
        val controller = OverviewController()
        controller.request("board", specs)
        controller.enterSketch("sketch", lane)
        assertFalse(controller.ui.value.open)
        controller.request("sketch", emptyList())
        assertTrue(controller.ui.value.open)
        assertEquals(specs, controller.ui.value.specs)
        assertEquals("board", controller.ui.value.boardPage)
    }

    @Test
    fun the_button_shows_on_a_board_and_on_the_sketch_page_opened_from_the_overview() {
        val controller = OverviewController()
        assertFalse(controller.showsButton("page", hasFrames = false))
        assertTrue(controller.showsButton("board", hasFrames = true))
        controller.request("board", specs)
        controller.enterSketch("sketch", lane)
        assertTrue(controller.showsButton("sketch", hasFrames = false))
        assertFalse(controller.showsButton("other", hasFrames = false))
    }

    @Test
    fun board_frames_update_the_copy_only_while_that_board_is_the_one_kept() {
        val controller = OverviewController()
        controller.request("board", specs)
        val moved = listOf(frame("live", 3000f))
        controller.boardFrames("board", moved)
        assertEquals(moved, controller.ui.value.specs)
        controller.boardFrames("elsewhere", specs)
        assertEquals(moved, controller.ui.value.specs)
        controller.boardFrames("board", emptyList())
        assertEquals(moved, controller.ui.value.specs)
    }

    @Test
    fun a_past_card_closes_the_overview_into_the_viewer_and_back_returns() {
        val controller = OverviewController()
        controller.request("board", specs)
        controller.view(lane, generation, "item-2-og")
        assertFalse(controller.ui.value.open)
        assertEquals(
            "item-2-og",
            controller.ui.value.viewer
                ?.shown,
        )
        controller.show("item-2-B")
        assertEquals(
            "item-2-B",
            controller.ui.value.viewer
                ?.shown,
        )
        controller.leaveViewer()
        assertTrue(controller.ui.value.open)
        assertNull(controller.ui.value.viewer)
    }

    @Test
    fun back_to_now_drops_the_viewer_without_reopening_the_overview() {
        val controller = OverviewController()
        controller.request("board", specs)
        controller.view(lane, generation, "item-2-og")
        controller.dropViewer()
        assertFalse(controller.ui.value.open)
        assertNull(controller.ui.value.viewer)
    }

    @Test
    fun the_viewer_switcher_is_og_then_the_variants_with_a_check_on_the_pick_and_no_use_this() {
        val state = HistorySwitcher.of(HistoryView(lane, generation, "item-2-B"))
        assertEquals(listOf("OG", "A", "B"), state.items.map { it.label })
        assertEquals(listOf(false, false, true), state.items.map { it.chosen })
        assertEquals("item-2-B", state.shown)
        assertFalse(state.canUseThis)
        assertEquals("item-2-A", state.neighbour(-1))
    }

    @Test
    fun the_viewer_shows_the_picture_of_the_selected_item_and_the_og_for_an_unknown_one() {
        assertEquals("asset-2-A", HistorySwitcher.picture(HistoryView(lane, generation, "item-2-A")).asset)
        assertEquals("asset-2-og", HistorySwitcher.picture(HistoryView(lane, generation, "nope")).asset)
    }
}
