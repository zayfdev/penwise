package dev.penwise.ink

import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.viewinterop.AndroidView

/** Hosts [InkSurfaceView] in Compose. */
@Composable
fun InkCanvas(
    store: StrokeStore,
    brushes: BrushSource,
    modifier: Modifier = Modifier,
) {
    AndroidView(
        factory = { context -> InkSurfaceView(context).apply { bind(store, brushes) } },
        modifier = modifier,
    )
}
