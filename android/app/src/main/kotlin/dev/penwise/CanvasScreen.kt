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
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import dev.penwise.frames.FrameSpec
import dev.penwise.frames.FrameStage
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
 * [frameSpecs] are live pages under the ink (M0: one, from the debug launch intent). With frames the paper and the
 * pages are window views below the surface, so the ink draws over them; the readout at the top left is the scroll the
 * page reported, which M2 needs to keep area marks on the document.
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
    frameSpecs: List<FrameSpec> = emptyList(),
) {
    val tools = page.tools
    val scene by page.scene.collectAsState()
    val open = scene ?: return
    val zoom by page.viewport.zoom.collectAsState()
    val style = PageStyle.of(pages.page(pages.active))
    ImmersiveWhileOpen()
    var confirmClear by rememberSaveable { mutableStateOf<String?>(null) }
    Box(modifier = Modifier.fillMaxSize()) {
        FrameStage(
            specs = frameSpecs,
            viewport = page.viewport,
            style = style,
            modifier = Modifier.fillMaxSize(),
        ) { frames ->
            InkCanvas(
                scene = open,
                brushes = tools,
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
                state = tools,
                expanded = drawerOpen,
                onToggle = { onDrawerOpenChange(!drawerOpen) },
                onUndo = page::undo,
                onRedo = page::redo,
                syncStatus = syncStatus,
                pending = pending,
                onSync = onSync,
            )
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
                        state = tools,
                        zoomPercent = (zoom * PERCENT).roundToInt(),
                        onFit = page.viewport::reset,
                        onClear = { confirmClear = Ids.hex(open.page) },
                        onSync = onSync,
                        onHome = onHome,
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
    }
    confirmClear?.let { forPage -> ClearPageDialog(forPage, open, page) { confirmClear = null } }
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
