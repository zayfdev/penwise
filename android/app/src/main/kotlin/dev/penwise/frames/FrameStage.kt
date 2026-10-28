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

/** The debug frame readout (M0): what the page under the ink reported. */
private const val READOUT_SP = 11
private const val READOUT_COLOR = 0xFF37474F
private const val READOUT_MARGIN_DP = 12

/**
 * The stage of the harness (§6.4): the paper and the live pages are window views below the ink, [content] puts the
 * canvas above them, and the readout at the top left shows what the page last reported (M2 moves area marks with it).
 * With no frames it composes the canvas alone and the surface keeps its opaque fast path.
 *
 * The WebViews are window views, so this must be composed before the chrome; the ink surface is above the whole window
 * and translucent while frames exist, which is what lets the pages and the readout show through it.
 */
@Composable
internal fun FrameStage(
    specs: List<FrameSpec>,
    viewport: ViewportState,
    style: PageStyle,
    modifier: Modifier = Modifier,
    content: @Composable (FrameHost?) -> Unit,
) {
    if (specs.isEmpty()) {
        content(null)
        return
    }
    val context = LocalContext.current
    val layer = remember { FrameLayer(context) }
    // Leaving the page destroys the WebViews (§M0.6): a frame that comes back loads its route again.
    DisposableEffect(layer) { onDispose { layer.release() } }
    var scroll by remember { mutableStateOf<FrameScroll?>(null) }
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
                it.setFrames(specs)
                it.onScroll = { _, reported -> scroll = reported }
            },
            modifier = Modifier.fillMaxSize(),
        )
        content(layer)
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

/** Debug readout of what the page under the ink reported (M0), visible through the translucent ink surface. */
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
