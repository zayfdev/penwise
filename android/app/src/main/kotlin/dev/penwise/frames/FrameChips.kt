package dev.penwise.frames

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import dev.penwise.R
import dev.penwise.ink.ViewTransform
import dev.penwise.ink.ViewportState
import dev.penwise.tools.ToolButton
import dev.penwise.tools.ToolChip
import dev.penwise.tools.ToolIcons
import kotlin.math.roundToInt

private const val CHIP_BACKGROUND = 0xF2FFFFFF
private const val CHIP_RADIUS_DP = 10
private const val CHIP_INSET_PX = 6
private const val CHIP_PAD_DP = 3
private const val CHIP_GAP_DP = 2

/**
 * The canvas controls of every frame, at the frame's top edge and outside the page: the preset switch, reload and the
 * route (which opens a small field). Screen-space: they keep their size while the canvas zooms, and they follow the
 * frames through [viewport].
 *
 * Composed above the ink surface (so Compose gives them the touch before the surface sees it), below it in z: the
 * translucent ink layer lets them show, ink drawn over them.
 */
@Composable
internal fun FrameChips(
    specs: List<FrameSpec>,
    viewport: ViewportState,
    layer: FrameLayer,
    onPreset: (id: String, preset: String) -> Unit,
    onRoute: (id: String, route: String) -> Unit,
) {
    var transform by remember { mutableStateOf(viewport.transform) }
    DisposableEffect(viewport) {
        val observer: (ViewTransform) -> Unit = { transform = it }
        viewport.observers.add(observer)
        onDispose { viewport.observers.remove(observer) }
    }
    val density = LocalDensity.current.density
    Box(Modifier.fillMaxSize()) {
        for (spec in specs) {
            key(spec.id) {
                val placement =
                    FrameGeometry.place(
                        FrameRect(spec.x, spec.y, spec.cssWidth.toFloat(), spec.cssHeight.toFloat()),
                        transform,
                        density,
                    )
                FrameChip(
                    spec = spec,
                    layer = layer,
                    onPreset = onPreset,
                    onRoute = onRoute,
                    modifier =
                        Modifier.offset {
                            IntOffset(
                                placement.left.roundToInt() + CHIP_INSET_PX,
                                placement.top.roundToInt() + CHIP_INSET_PX,
                            )
                        },
                )
            }
        }
    }
}

/** One frame's controls: the preset (tap cycles), reload, and the route it shows (tap to open another). */
@Composable
private fun FrameChip(
    spec: FrameSpec,
    layer: FrameLayer,
    onPreset: (id: String, preset: String) -> Unit,
    onRoute: (id: String, route: String) -> Unit,
    modifier: Modifier = Modifier,
) {
    var editingRoute by remember { mutableStateOf(false) }
    Row(
        modifier
            .background(Color(CHIP_BACKGROUND), RoundedCornerShape(CHIP_RADIUS_DP.dp))
            .padding(CHIP_PAD_DP.dp),
        horizontalArrangement = Arrangement.spacedBy(CHIP_GAP_DP.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        ToolChip(
            label = spec.preset,
            selected = false,
            onClick = { onPreset(spec.id, FramePreset.nextFrom(spec.preset).key) },
        )
        ToolButton(
            icon = ToolIcons.Sync,
            label = stringResource(R.string.frame_reload),
            selected = false,
            onClick = { layer.reload(spec.id) },
        )
        ToolChip(label = spec.route, selected = false, onClick = { editingRoute = true })
    }
    if (editingRoute) {
        RouteDialog(
            current = spec.route,
            onDismiss = { editingRoute = false },
            onOpen = { route ->
                editingRoute = false
                onRoute(spec.id, route)
            },
        )
    }
}
