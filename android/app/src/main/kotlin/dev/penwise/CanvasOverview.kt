package dev.penwise

import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import dev.penwise.frames.AddFrameDialog
import dev.penwise.frames.FrameLane
import dev.penwise.frames.FrameServer
import dev.penwise.ink.PageList
import dev.penwise.ink.PageTemplate
import dev.penwise.ink.PageViewModel
import dev.penwise.overview.CardImage
import dev.penwise.overview.DeckState
import dev.penwise.overview.HistoryViewer
import dev.penwise.overview.OverviewButton
import dev.penwise.overview.OverviewCommands
import dev.penwise.overview.OverviewImages
import dev.penwise.overview.OverviewPort
import dev.penwise.overview.OverviewRemote
import dev.penwise.overview.OverviewStrips
import dev.penwise.overview.OverviewUi
import dev.penwise.overview.OverviewWindow
import dev.penwise.overview.OverviewWindowActions
import dev.penwise.overview.OverviewWindowModel
import dev.penwise.overview.ViewerActions
import dev.penwise.overview.rememberReducedMotion

/** How long opening the overview waits for the "now" card's picture, so the opening animation has one to show. */
private const val PRELOAD_MS = 600L

/** The overview's needs from the app, answered by the view model. */
private class PagePort(
    private val page: PageViewModel,
) : OverviewPort {
    override val activePage: String get() = page.pages.value.active
    override val pages: PageList get() = page.pages.value

    override fun selectPage(id: String) = page.selectPage(id)

    override fun lockFrame(frame: String) = page.frameControl.lockTo(frame)

    override fun createSketchPage(
        notebook: String,
        title: String,
        created: (String) -> Unit,
    ) = page.catalog.newPage(title, PageTemplate.Infinite, notebook, created)

    override fun addFrame(
        project: String,
        route: String,
        preset: String,
        cssW: Int,
        cssH: Int,
        x: Float,
        y: Float,
    ) = page.addFrame(project, route, preset, cssW, cssH, x, y)

    override fun refreshProjects() = page.refreshProjects()
}

/**
 * The overview around the open page (locked-frames contract §3.3): the overview button in its own chrome window, the
 * overview window, the read-only viewer of a past picture and the "New screen" dialog. Wiring only: the state is
 * [PageViewModel.overview], the taps are [OverviewCommands], the strips are built by `OverviewStrips`.
 */
@Composable
internal fun OverviewHost(
    page: PageViewModel,
    pages: PageList,
) {
    val ui by page.overview.ui.collectAsState()
    val frames by page.frames.collectAsState()
    val server by page.frameSession.server.collectAsState()
    val latestServer by rememberUpdatedState(server)
    // Kept while the screen is: reopening the overview finds its pictures in memory.
    val images = remember { OverviewImages { latestServer } }
    val commands = remember(page) { OverviewCommands(page.overview, PagePort(page)) }
    var newScreen by remember { mutableStateOf(false) }
    // The board's frames are copied while it is on screen, so a sketch page can reopen the overview later.
    LaunchedEffect(frames, pages.active) { page.overview.boardFrames(pages.active, frames) }
    OverviewButton(visible = page.overview.showsButton(pages.active, frames.isNotEmpty()), onClick = page::openOverview)
    if (ui.open) OverviewSession(page, pages, ui, server, images, commands) { newScreen = true }
    ui.viewer?.let { view ->
        HistoryViewer(
            view = view,
            images = images,
            canGoNow = FrameLane.live(ui.specs, view.lane) != null,
            actions = ViewerActions(page.overview::show, { commands.backToNow(view) }, page.overview::leaveViewer),
        )
    }
    if (newScreen) NewScreenDialog(page, commands) { newScreen = false }
}

@Composable
private fun OverviewSession(
    page: PageViewModel,
    pages: PageList,
    ui: OverviewUi,
    server: FrameServer?,
    images: OverviewImages,
    commands: OverviewCommands,
    onNewScreen: () -> Unit,
) {
    var decks by remember { mutableStateOf(DeckState()) }
    var remote by remember { mutableStateOf(OverviewRemote()) }
    val attached by page.overview.attached.collectAsState()
    LaunchedEffect(Unit) { commands.opened() }
    // Refetched when the overview opens, when a round resolves (the frames move) and when a sketch page got attached.
    LaunchedEffect(server, OverviewStrips.generationKey(ui.specs), attached) {
        val reachable = server ?: return@LaunchedEffect
        remote = commands.remote(reachable, ui.specs)
    }
    val strips =
        OverviewStrips.of(
            ui.specs,
            remote.history,
            remote.stripPages,
            remote.openRounds,
        ) { pages.page(it)?.title }
    val focus = OverviewStrips.indexOf(strips, ui.lane)
    val now = strips.getOrNull(focus)?.live?.id
    // The window opens once the "now" card's picture is in hand, so the opening animation has something to shrink.
    var seed by remember { mutableStateOf<Map<String, ImageBitmap>?>(null) }
    LaunchedEffect(now) {
        if (now != null && seed == null) {
            val bitmap = images.preload(CardImage.Still(now), PRELOAD_MS)
            seed = bitmap?.let { mapOf(now to it.asImageBitmap()) }.orEmpty()
        }
    }
    val ready = seed
    if (strips.isEmpty() || ready == null) return
    val model =
        OverviewWindowModel(strips, focus, decks, images::card, images::peek, ready, rememberReducedMotion())
    val actions =
        OverviewWindowActions(
            onDismiss = page.overview::close,
            onToggleDeck = { lane, column -> decks = decks.toggled(lane, column) },
            onEnter = commands::enter,
            onPlus = commands::plus,
            onNewScreen = onNewScreen,
        )
    OverviewWindow(model, actions)
}

/** The existing add-frame flow; the overview reopens after it. */
@Composable
private fun NewScreenDialog(
    page: PageViewModel,
    commands: OverviewCommands,
    onDone: () -> Unit,
) {
    val projects by page.projects.collectAsState()
    LaunchedEffect(Unit) { commands.prepareNewScreen() }
    AddFrameDialog(
        projects = projects,
        onDismiss = {
            onDone()
            commands.reopen()
        },
        onAdd = { project, route, preset ->
            onDone()
            commands.addScreen(project, route, preset)
        },
    )
}
