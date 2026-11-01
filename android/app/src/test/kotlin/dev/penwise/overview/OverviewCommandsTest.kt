package dev.penwise.overview

import dev.penwise.frames.LaneKey
import dev.penwise.ink.PageBook
import dev.penwise.ink.PageList
import dev.penwise.ink.PageMeta
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** What the overview's taps do, against a fake app: page switches, locks, new pages and new screens. */
class OverviewCommandsTest {
    private val lane = LaneKey("p", "/", "desktop")
    private val board = "b0a4d000000000000000000000000001"
    private val specs = listOf(frame("live", 0f), frame("low", 0f, y = 2000f, preset = "phone"))

    private class Port(
        override var activePage: String,
        override val pages: PageList,
    ) : OverviewPort {
        val calls = ArrayList<String>()
        var created: ((String) -> Unit)? = null

        override fun selectPage(id: String) {
            calls += "select $id"
            activePage = id
        }

        override fun lockFrame(frame: String) {
            calls += "lock $frame"
        }

        override fun createSketchPage(
            notebook: String,
            title: String,
            created: (String) -> Unit,
        ) {
            calls += "create $notebook $title"
            this.created = created
        }

        override fun addFrame(
            project: String,
            route: String,
            preset: String,
            cssW: Int,
            cssH: Int,
            x: Float,
            y: Float,
        ) {
            calls += "add $project $route $preset ${cssW}x$cssH @${x.toInt()},${y.toInt()}"
        }

        override fun refreshProjects() {
            calls += "projects"
        }
    }

    private fun pages(vararg titles: String) =
        PageList(
            board,
            listOf(
                PageBook(
                    "nb",
                    "Notebook",
                    listOf(PageMeta(board, "Board", infinite = true, background = "blank")) +
                        titles.mapIndexed { i, t -> PageMeta("s$i", t, infinite = true, background = "blank") },
                ),
            ),
        )

    private val controller = OverviewController()

    private fun setup(
        active: String = board,
        pages: PageList = pages(),
    ): Pair<OverviewCommands, Port> {
        val port = Port(active, pages)
        controller.request(board, specs, locked = "live")
        return OverviewCommands(controller, port) { _, _ -> OverviewRemote() } to port
    }

    @Test
    fun entering_a_frame_on_the_board_locks_it_and_closes_the_overview() {
        val (commands, port) = setup()
        commands.enter(CardAction.LockFrame("live"))
        assertEquals(listOf("lock live"), port.calls)
        assertFalse(controller.ui.value.open)
    }

    @Test
    fun entering_a_frame_from_a_sketch_page_selects_the_board_first() {
        val (commands, port) = setup(active = "sketch")
        commands.enter(CardAction.LockFrame("live"))
        assertEquals(listOf("select $board", "lock live"), port.calls)
    }

    @Test
    fun a_sketch_card_opens_that_page_and_remembers_its_strip() {
        val (commands, port) = setup()
        commands.enter(CardAction.OpenSketch(SKETCH_1, lane))
        assertEquals(listOf("select $SKETCH_1"), port.calls)
        assertEquals(SKETCH_1, controller.ui.value.sketch)
        assertEquals(lane, controller.ui.value.lane)
        assertFalse(controller.ui.value.open)
    }

    @Test
    fun a_history_card_opens_the_viewer() {
        val (commands, _) = setup()
        val generation = generation(2, "A" to true)
        commands.enter(CardAction.ViewHistory(lane, generation, "item-2-A"))
        assertEquals(
            "item-2-A",
            controller.ui.value.viewer
                ?.shown,
        )
    }

    @Test
    fun back_to_now_locks_the_lanes_live_frame_and_drops_the_viewer() {
        val (commands, port) = setup()
        val view = HistoryView(lane, generation(2), "item-2-og")
        controller.view(lane, view.generation, view.shown)
        commands.backToNow(view)
        assertEquals(listOf("lock live"), port.calls)
        assertNull(controller.ui.value.viewer)
    }

    @Test
    fun a_new_sketch_page_is_named_after_the_notebooks_pages_and_opened_in_its_strip() {
        val (commands, port) = setup(pages = pages("Sketch 1"))
        commands.plus(PlusAction.NewSketchPage(lane))
        assertEquals(listOf("create nb Sketch 2"), port.calls)
        port.created?.invoke(SKETCH_2)
        assertEquals(SKETCH_2, controller.ui.value.sketch)
        assertEquals(lane, controller.ui.value.lane)
        assertNull(controller.sketches)
    }

    @Test
    fun a_new_screen_selects_the_board_page_and_goes_under_the_lowest_frame() {
        val (commands, port) = setup(active = "sketch")
        commands.prepareNewScreen()
        commands.addScreen("p", "/pricing", "tablet")
        // The lowest frame ends at y 2000 + 900, and the gap is 160.
        assertEquals(listOf("select $board", "projects", "add p /pricing tablet 834x1194 @0,3060"), port.calls)
        assertTrue(controller.ui.value.open)
    }

    @Test
    fun cancelling_the_new_screen_dialog_reopens_the_overview() {
        val (commands, _) = setup()
        controller.close()
        commands.reopen()
        assertTrue(controller.ui.value.open)
    }

    @Test
    fun the_remote_fetch_covers_every_lane_of_the_board_once() {
        var asked: List<LaneKey> = emptyList()
        val port = Port(board, pages())
        controller.request(board, specs + frame("old", -9f), locked = "live")
        val commands = OverviewCommands(controller, port) { _, lanes -> OverviewRemote().also { asked = lanes } }
        kotlinx.coroutines.runBlocking {
            commands.remote(dev.penwise.frames.FrameServer("http://x", ""), controller.ui.value.specs)
        }
        assertEquals(listOf(lane, LaneKey("p", "/", "phone")), asked)
    }
}
