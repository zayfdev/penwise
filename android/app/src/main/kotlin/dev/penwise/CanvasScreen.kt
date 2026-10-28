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
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.BiasAlignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import dev.penwise.ink.InkCanvas
import dev.penwise.ink.PageList
import dev.penwise.ink.PageStrip
import dev.penwise.ink.PageStyle
import dev.penwise.ink.PageViewModel
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
 * are a drawer under it ([drawerOpen]). The pen touching the page closes the drawer.
 */
@Composable
fun CanvasScreen(
    pages: PageList,
    page: PageViewModel,
    drawerOpen: Boolean,
    onDrawerOpenChange: (Boolean) -> Unit,
    onHome: () -> Unit,
    onSync: () -> Unit,
) {
    val tools = page.tools
    val scene by page.scene.collectAsState()
    val open = scene ?: return
    val zoom by page.viewport.zoom.collectAsState()
    ImmersiveWhileOpen()
    var confirmClear by remember { mutableStateOf(false) }
    Box(modifier = Modifier.fillMaxSize()) {
        InkCanvas(
            scene = open,
            brushes = tools,
            viewport = page.viewport,
            style = PageStyle.of(pages.page(pages.active)),
            images = page.images,
            liveOut = page.liveOut,
            onPenDown = { onDrawerOpenChange(false) },
            modifier = Modifier.fillMaxSize(),
        )
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
                onUndo = { open.undo() },
                onRedo = { open.redo() },
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
                        onClear = { confirmClear = true },
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
    if (confirmClear) {
        ConfirmDialog(
            title = stringResource(R.string.clear_page_title),
            message = stringResource(R.string.clear_page_message),
            confirm = stringResource(R.string.tool_clear_page),
            destructive = true,
            onDismiss = { confirmClear = false },
            onConfirm = {
                confirmClear = false
                open.clearPage()
            },
        )
    }
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
