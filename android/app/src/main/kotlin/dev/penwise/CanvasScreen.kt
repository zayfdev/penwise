package dev.penwise

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.systemGestureExclusion
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.BiasAlignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.dp
import dev.penwise.frames.AddFrameDialog
import dev.penwise.frames.DesignProject
import dev.penwise.frames.FramePreset
import dev.penwise.frames.FrameStage
import dev.penwise.frames.FrameStageActions
import dev.penwise.frames.FrameStageModel
import dev.penwise.ink.InkCanvas
import dev.penwise.ink.PageList
import dev.penwise.ink.PageScene
import dev.penwise.ink.PageStrip
import dev.penwise.ink.PageStyle
import dev.penwise.ink.PageViewModel
import dev.penwise.sync.Ids
import dev.penwise.sync.SyncStatus
import dev.penwise.tools.ConfirmDialog
import dev.penwise.tools.MiniBar
import dev.penwise.tools.Toolbar
import kotlin.math.roundToInt

private const val CHROME_MARGIN_DP = 12
private const val PERCENT = 100

/** Edge strip kept clear of the system back gesture. Android honours at most 200dp of height per edge. */
private const val EXCLUSION_WIDTH_DP = 48
private const val EXCLUSION_HEIGHT_DP = 200

/** Vertical bias (-1 top … 1 bottom) of the exclusion strips: centre-lower, where the writing hand rests. */
private const val EXCLUSION_BIAS = 0.45f

/**
 * An open page: the canvas fills the screen and the only chrome is the [MiniBar]; the full tools and the page strip
 * are a drawer under it ([drawerOpen]). The pen touching the page closes the drawer. "Clear page" confirms for the
 * page it was opened on (its id survives recreation); if another page is showing by then it does nothing.
 *
 * The page's frames (synced items) are live pages under the ink. With frames the paper and the pages are window views
 * below the surface, so the ink draws over them; each frame carries its own controls at its top edge, and the drawer
 * adds one at the centre of the view.
 */
@Composable
fun CanvasScreen(
    pages: PageList,
    page: PageViewModel,
    drawerOpen: Boolean,
    onDrawerOpenChange: (Boolean) -> Unit,
    onHome: () -> Unit,
    onSync: () -> Unit,
    syncStatus: SyncStatus,
    pending: Int,
) {
    val scene by page.scene.collectAsState()
    val open = scene ?: return
    val style = PageStyle.of(pages.page(pages.active))
    val frameSpecs by page.frames.collectAsState()
    val projects by page.projects.collectAsState()
    val marks by page.marks.collectAsState()
    val watchKeys by page.watchKeys.collectAsState()
    val showDone by page.showDone.collectAsState()
    var canvasSize by remember { mutableStateOf(IntSize.Zero) }
    var showAddFrame by rememberSaveable { mutableStateOf(false) }
    ImmersiveWhileOpen()
    var confirmClear by rememberSaveable { mutableStateOf<String?>(null) }
    // The chrome's callbacks in one value, so the drawer's signature stays short.
    val actions =
        remember(open.page) {
            CanvasActions(
                onClear = { confirmClear = Ids.hex(open.page) },
                onAddFrame = {
                    showAddFrame = true
                    page.refreshProjects()
                },
                onSync = onSync,
                onHome = onHome,
            )
        }
    val stageActions =
        FrameStageActions(
            onPreset = page::setFramePreset,
            onRoute = page::setFrameRoute,
            onLayout = page::setFrameLayout,
            onMarkStatus = page::setMarkStatus,
        )
    Box(modifier = Modifier.fillMaxSize().onSizeChanged { canvasSize = it }) {
        FrameStage(
            model = FrameStageModel(frameSpecs, marks, watchKeys, stageActions),
            viewport = page.viewport,
            style = style,
            modifier = Modifier.fillMaxSize(),
        ) { frames ->
            InkCanvas(
                scene = open,
                brushes = page.tools,
                viewport = page.viewport,
                style = style,
                images = page.images,
                liveOut = page.liveOut,
                pen = page.pen,
                frames = frames,
                onPenDown = { onDrawerOpenChange(false) },
                modifier = Modifier.fillMaxSize(),
            )
        }
        GestureExclusionStrips()
        Column(
            modifier =
                Modifier
                    .align(Alignment.TopCenter)
                    .safeDrawingPadding()
                    .padding(CHROME_MARGIN_DP.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(CHROME_MARGIN_DP.dp),
        ) {
            MiniBar(
                state = page.tools,
                expanded = drawerOpen,
                onToggle = { onDrawerOpenChange(!drawerOpen) },
                onUndo = page::undo,
                onRedo = page::redo,
                syncStatus = syncStatus,
                pending = pending,
                onSync = onSync,
            )
            CanvasDrawer(pages, page, drawerOpen, showDone, actions)
        }
    }
    confirmClear?.let { forPage -> ClearPageDialog(forPage, open, page) { confirmClear = null } }
    if (showAddFrame) {
        NewFrameDialog(projects, page, canvasSize) { showAddFrame = false }
    }
}

/** The canvas chrome's callbacks, one value so the drawer takes few parameters. */
private class CanvasActions(
    val onClear: () -> Unit,
    val onAddFrame: () -> Unit,
    val onSync: () -> Unit,
    val onHome: () -> Unit,
)

/** The drawer under the mini bar: the full tool panel and the page strip, sliding in while [drawerOpen]. */
@Composable
private fun CanvasDrawer(
    pages: PageList,
    page: PageViewModel,
    drawerOpen: Boolean,
    showDone: Boolean,
    actions: CanvasActions,
) {
    val zoom by page.viewport.zoom.collectAsState()
    AnimatedVisibility(
        visible = drawerOpen,
        enter = expandVertically(expandFrom = Alignment.Top) + fadeIn(),
        exit = shrinkVertically(shrinkTowards = Alignment.Top) + fadeOut(),
    ) {
        Column(
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(CHROME_MARGIN_DP.dp),
        ) {
            Toolbar(
                state = page.tools,
                zoomPercent = (zoom * PERCENT).roundToInt(),
                showDone = showDone,
                onShowDone = { page.setShowDone(!showDone) },
                onFit = page.viewport::reset,
                onClear = actions.onClear,
                onSync = actions.onSync,
                onHome = actions.onHome,
                onAddFrame = actions.onAddFrame,
            )
            PageStrip(
                pages = pages,
                activeId = pages.active,
                onSelect = page::selectPage,
                onNew = { title, template -> page.catalog.newPage(title, template) },
                onRename = page.catalog::renamePage,
                onDelete = page.catalog::deletePage,
            )
        }
    }
}

/** The new-frame dialog; the frame lands at the centre of the view, its top-left placed in page px. */
@Composable
private fun NewFrameDialog(
    projects: List<DesignProject>,
    page: PageViewModel,
    canvasSize: IntSize,
    onDismiss: () -> Unit,
) {
    AddFrameDialog(
        projects = projects,
        onDismiss = onDismiss,
        onAdd = { project, route, preset ->
            onDismiss()
            val size = FramePreset.byKey(preset) ?: FramePreset.PHONE
            val transform = page.viewport.transform
            // The frame's page box is css_w × css_h page units (C1): centred on the visible page point.
            page.addFrame(
                project = project,
                route = route,
                preset = size.key,
                cssW = size.cssWidth,
                cssH = size.cssHeight,
                x = transform.toPageX(canvasSize.width / 2f) - size.cssWidth / 2f,
                y = transform.toPageY(canvasSize.height / 2f) - size.cssHeight / 2f,
            )
        },
    )
}

/** Asks before clearing; confirms only for the page it was opened on. */
@Composable
private fun ClearPageDialog(
    forPage: String,
    open: PageScene,
    page: PageViewModel,
    onDone: () -> Unit,
) {
    ConfirmDialog(
        title = stringResource(R.string.clear_page_title),
        message = stringResource(R.string.clear_page_message),
        confirm = stringResource(R.string.tool_clear_page),
        destructive = true,
        onDismiss = onDone,
        onConfirm = {
            onDone()
            if (Ids.hex(open.page) == forPage) page.clearPage(open)
        },
    )
}

/**
 * Asks the system to leave the left and right screen edges to the app (so a swipe from the edge while writing is
 * not taken as Back). The platform caps this at 200dp of height per edge, so the strips sit centre-lower where the
 * hand rests. The bottom home/recents swipe cannot be excluded; immersive mode turns it into a peek of the bars.
 * The strips carry no pointer handling, so touches pass through to the canvas.
 */
@Composable
private fun BoxScope.GestureExclusionStrips() {
    val strip = Modifier.size(EXCLUSION_WIDTH_DP.dp, EXCLUSION_HEIGHT_DP.dp).systemGestureExclusion()
    Box(strip.align(BiasAlignment(-1f, EXCLUSION_BIAS)))
    Box(strip.align(BiasAlignment(1f, EXCLUSION_BIAS)))
}
