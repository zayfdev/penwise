package dev.penwise.overview

import dev.penwise.frames.FrameLane
import dev.penwise.frames.FramePreset
import dev.penwise.frames.FrameServer
import dev.penwise.frames.FrameSpec
import dev.penwise.frames.LaneKey
import dev.penwise.frames.box
import dev.penwise.frames.lane
import dev.penwise.ink.PageList
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/** What the overview's commands need from the app: pages, the locked frame, new pages and frames. */
internal interface OverviewPort {
    /** The page on screen. */
    val activePage: String
    val pages: PageList

    fun selectPage(id: String)

    /** Locks the board's camera to [frame] now, or as soon as the board shows it. */
    fun lockFrame(frame: String)

    /** Creates an Infinite page titled [title] in [notebook]; [created] gets its id. */
    fun createSketchPage(
        notebook: String,
        title: String,
        created: (String) -> Unit,
    )

    @Suppress("LongParameterList")
    fun addFrame(
        project: String,
        route: String,
        preset: String,
        cssW: Int,
        cssH: Int,
        x: Float,
        y: Float,
    )

    fun refreshProjects()
}

/**
 * What the overview's taps do (contract §3.3): entering a card, `+`, "New screen", the viewer's "Back to now". The
 * composables only call these; the state lives in [OverviewController].
 */
internal class OverviewCommands(
    private val ui: OverviewController,
    private val port: OverviewPort,
    private val fetchRemote: (FrameServer, List<LaneKey>) -> OverviewRemote = OverviewRemote::fetch,
) {
    /** The overview opened: sketch pages that could not be attached before are sent again. */
    fun opened() {
        ui.sketches?.flush()
    }

    /** What the server keeps for the strips of [specs]' lanes, fetched off the main thread. */
    suspend fun remote(
        server: FrameServer,
        specs: List<FrameSpec>,
    ): OverviewRemote = withContext(Dispatchers.IO) { fetchRemote(server, specs.map { it.lane }.distinct()) }

    /** A tap on a card. */
    fun enter(action: CardAction) {
        when (action) {
            is CardAction.LockFrame -> {
                enterFrame(action.frameId)
            }

            is CardAction.ViewHistory -> {
                ui.view(action.lane, action.generation, action.item)
            }

            is CardAction.OpenSketch -> {
                port.selectPage(action.page)
                ui.enterSketch(action.page, action.lane)
            }
        }
    }

    /** Locks the board to [frame]: the board page first when a sketch page is open. */
    fun enterFrame(frame: String) {
        ui.ui.value.boardPage
            ?.takeIf { it != port.activePage }
            ?.let(port::selectPage)
        port.lockFrame(frame)
        ui.close()
    }

    /** The viewer's "Back to now": the lane's live frame is locked. */
    fun backToNow(view: HistoryView) {
        ui.dropViewer()
        FrameLane.live(ui.ui.value.specs, view.lane)?.let { enterFrame(it.id) }
    }

    /** A pick in a strip's `+` menu. */
    fun plus(action: PlusAction) {
        when (action) {
            is PlusAction.NewSketchPage -> newSketch(action.lane)
        }
    }

    /**
     * An Infinite page "Sketch n" in the board page's notebook, opened at once and attached to [lane]'s strip on the
     * server by the [SketchAttacher] (which waits for sync and keeps retrying).
     */
    private fun newSketch(lane: LaneKey) {
        val board = ui.ui.value.boardPage
        val book = port.pages.books.firstOrNull { book -> book.pages.any { it.id == board } } ?: return
        val title = PlusMenu.nextSketchTitle(book.pages.map { it.title })
        port.createSketchPage(book.id, title) { created ->
            ui.enterSketch(created, lane)
            ui.sketches?.add(lane, created)
        }
    }

    /** "New screen" was tapped: the frame is added to the board page, wherever the owner is now. */
    fun prepareNewScreen() {
        ui.ui.value.boardPage
            ?.takeIf { it != port.activePage }
            ?.let(port::selectPage)
        port.refreshProjects()
    }

    /** The add-frame dialog was filled in: the new screen goes under the board's lowest frame. */
    fun addScreen(
        project: String,
        route: String,
        preset: String,
    ) {
        val size = FramePreset.byKey(preset) ?: FramePreset.PHONE
        val (x, y) =
            NewScreenPlacement.at(
                ui.ui.value.specs
                    .map { it.box },
            )
        port.addFrame(project, route, size.key, size.cssWidth, size.cssHeight, x, y)
        reopen()
    }

    /** The overview comes back after the new-screen dialog (added or cancelled). */
    fun reopen() = ui.request(port.activePage, emptyList())
}
