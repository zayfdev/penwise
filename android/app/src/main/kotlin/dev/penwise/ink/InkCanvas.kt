package dev.penwise.ink

import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.viewinterop.AndroidView

/** Hosts [InkSurfaceView] in Compose. */
@Composable
fun InkCanvas(
    store: StrokeStore,
    brushes: BrushSource,
    style: PageStyle,
    modifier: Modifier = Modifier,
    liveOut: LiveOut? = null,
) {
    AndroidView(
        factory = { context ->
            InkSurfaceView(context).apply {
                bind(store, brushes, liveOut)
                setPageStyle(style)
            }
        },
        update = { view -> view.setPageStyle(style) },
        modifier = modifier,
    )
}
