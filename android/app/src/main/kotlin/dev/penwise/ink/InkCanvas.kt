package dev.penwise.ink

import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.viewinterop.AndroidView

/** Hosts [InkSurfaceView] in Compose, showing [scene] (switch pages by passing another scene). */
@Composable
fun InkCanvas(
    scene: PageScene,
    brushes: BrushSource,
    style: PageStyle,
    modifier: Modifier = Modifier,
    images: PageImages? = null,
    liveOut: LiveOut? = null,
) {
    AndroidView(
        factory = { context ->
            InkSurfaceView(context).apply {
                bind(scene, brushes, images, liveOut)
                setPageStyle(style)
            }
        },
        update = { view ->
            view.showScene(scene)
            view.setPageStyle(style)
        },
        modifier = modifier,
    )
}
