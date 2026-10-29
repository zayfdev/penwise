package dev.penwise.frames

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.viewinterop.AndroidView
import dev.penwise.ink.FrameHost
import dev.penwise.ink.PageStyle
import dev.penwise.ink.PaperView
import dev.penwise.ink.ViewportState

private const val READOUT_SP = 11
private const val READOUT_COLOR = 0xFF37474F
private const val READOUT_MARGIN_DP = 12

/** What the frame stage shows and the calls that change it; one value so [FrameStage]'s signature stays short. */
internal class FrameStageModel(
    val specs: List<FrameSpec>,
    val marks: List<SceneMark>,
    val watchKeys: Map<String, List<String>>,
    val actions: FrameStageActions,
)

/** The stage's callbacks: frame item edits and the core's layout feed and mark status. */
internal class FrameStageActions(
    val onPreset: (id: String, preset: String) -> Unit,
    val onRoute: (id: String, route: String) -> Unit,
    val onLayout: (id: String, json: String) -> Unit,
    val onMarkStatus: (id: String, status: String) -> Unit,
)

/**
 * The stage of the harness (§6.4): the paper and the live pages are window views below the ink, [content] puts the
 * canvas above them, and the frames' own controls sit on top of the canvas but outside the pages. With no frames it
 * composes the canvas alone and the surface keeps its opaque fast path.
 *
 * The WebViews are window views, so this must be composed before the chrome; the ink surface is above the whole window
 * and translucent while frames exist, which is what lets the pages and the controls show through it.
 *
 * Every `pw:layout` goes to the core (M2Core owns placement); the frames' watched keys are re-sent after the core
 * reports them, and the mark chips follow the core's mark list.
 */
@Composable
internal fun FrameStage(
    model: FrameStageModel,
    viewport: ViewportState,
    style: PageStyle,
    modifier: Modifier = Modifier,
    content: @Composable (FrameHost?) -> Unit,
) {
    if (model.specs.isEmpty()) {
        content(null)
        return
    }
    val context = LocalContext.current
    val layer = remember { FrameLayer(context) }
    // Leaving the page destroys the WebViews: a frame that comes back loads its route again.
    DisposableEffect(layer) { onDispose { layer.release() } }
    var scroll by remember { mutableStateOf<FrameScroll?>(null) }
    // Each frame's current state recipe: a mark drawn in another one is dimmed and offers a replay.
    val states = remember { mutableStateOf<Map<String, String>>(emptyMap()) }
    Box(modifier = modifier) {
        AndroidView(
            factory = { PaperView(it) },
            update = { it.bind(viewport, style) },
            modifier = Modifier.fillMaxSize(),
        )
        AndroidView(
            factory = { layer },
            update = {
                it.bind(viewport)
                it.setFrames(model.specs)
                it.setWatch(model.watchKeys)
                it.onScroll = { _, reported -> scroll = reported }
                it.onRoute = model.actions.onRoute
                it.onLayout = { id, json, state ->
                    model.actions.onLayout(id, json)
                    if (states.value[id] != state) states.value = states.value + (id to state)
                }
            },
            modifier = Modifier.fillMaxSize(),
        )
        content(layer)
        FrameChips(
            specs = model.specs,
            viewport = viewport,
            layer = layer,
            onPreset = model.actions.onPreset,
            onRoute = model.actions.onRoute,
        )
        FrameMarkChips(
            marks = model.marks,
            specs = model.specs,
            viewport = viewport,
            layer = layer,
            states = states.value,
            onStatus = model.actions.onMarkStatus,
        )
        FrameReadout(
            scroll = scroll,
            modifier =
                Modifier
                    .align(Alignment.TopStart)
                    .safeDrawingPadding()
                    .padding(READOUT_MARGIN_DP.dp),
        )
    }
}

/** Readout of what the page under the ink last reported (M2 moves area marks with it). */
@Composable
private fun FrameReadout(
    scroll: FrameScroll?,
    modifier: Modifier = Modifier,
) {
    val text =
        if (scroll == null) {
            "frame: loading"
        } else {
            "frame: scroll ${scroll.x}, ${scroll.y} · doc ${scroll.docWidth}×${scroll.docHeight}"
        }
    BasicText(
        text = text,
        style = TextStyle(fontSize = READOUT_SP.sp, color = Color(READOUT_COLOR)),
        modifier = modifier,
    )
}
