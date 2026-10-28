package dev.penwise.ink

import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.viewinterop.AndroidView

/**
 * Hosts [InkSurfaceView] in Compose, showing [scene] (switch pages by passing another scene) through [viewport].
 * [onPenDown] runs when the stylus touches the canvas.
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
    onPenDown: () -> Unit = {},
) {
    AndroidView(
        factory = { context ->
            InkSurfaceView(context).apply {
                bind(scene, brushes, viewport, images, liveOut, pen)
                setPageStyle(style)
            }
        },
        update = { view ->
            view.onPenDown = onPenDown
            view.showScene(scene)
            view.setPageStyle(style)
        },
        modifier = modifier,
    )
}
