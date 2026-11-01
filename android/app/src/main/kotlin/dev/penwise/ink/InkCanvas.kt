package dev.penwise.ink

import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.viewinterop.AndroidView

/**
 * Hosts [InkSurfaceView] in Compose, showing [scene] (switch pages by passing another scene) through [viewport].
 * [frames], when given, are live pages under the ink: the surface then goes translucent above the window and one
 * finger on a page belongs to it. [onPenDown] runs when the stylus touches the canvas; [onPenInput] on every stylus
 * touch or hover event.
 */
@Composable
fun InkCanvas(
    scene: PageScene,
    brushes: BrushSource,
    viewport: ViewportState,
    style: PageStyle,
    modifier: Modifier = Modifier,
    images: PageImages? = null,
    liveOut: LiveOut? = null,
    pen: PenActivity = PenActivity(),
    frames: FrameHost? = null,
    onPenDown: () -> Unit = {},
    onPenInput: PenInputListener = PenInputListener {},
) {
    AndroidView(
        factory = { context ->
            InkSurfaceView(context).apply {
                bind(scene, brushes, viewport, images, liveOut, pen)
                setPageStyle(style)
                // Before the view is attached: the surface's z-order and format are fixed for its life. Switching them
                // later recreates the surface under the low-latency renderer, which then never draws again (Tab S9:
                // no ink on a board). The stage composes a new canvas when frames come or go.
                setFrames(frames)
            }
        },
        update = { view ->
            view.onPenDown = onPenDown
            view.onPenInput = onPenInput
            view.showScene(scene)
            view.setPageStyle(style)
            view.setFrames(frames)
        },
        modifier = modifier,
    )
}
