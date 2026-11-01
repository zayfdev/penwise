package dev.penwise.frames

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
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
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.penwise.R
import dev.penwise.ink.ViewTransform
import dev.penwise.ink.ViewportState
import dev.penwise.tools.ToolButton
import dev.penwise.tools.ToolChip
import dev.penwise.tools.ToolIcons
import kotlinx.coroutines.delay
import kotlin.math.roundToInt

private const val CHIP_BACKGROUND = 0xF2FFFFFF
private const val CHIP_RADIUS_DP = 10
private const val CHIP_INSET_PX = 6
private const val CHIP_PAD_DP = 3
private const val CHIP_GAP_DP = 2

/** The camera must be still this long before the chip's window shows (and moves) again. */
private const val REST_MS = 160L

/** The chip row's callbacks: the frame item edits and the Ghost toggle. */
private class FrameChipActions(
    val onPreset: (id: String, preset: String) -> Unit,
    val onRoute: (id: String, route: String) -> Unit,
    val onGhost: (id: String, on: Boolean) -> Unit,
)

/**
 * The canvas controls of the locked frame, at its top-left corner and outside the page: the preset switch, reload, the
 * route (which opens a small field) and the Ghost toggle of a patched frame. A variant's name and pick live in the
 * [VariantSwitcher], not here. With the camera locked to one frame no other frame is on the screen, so one chip is
 * enough.
 *
 * Screen-space: it keeps its size while the canvas zooms and follows the frame through [viewport] (staying inside the
 * screen when the frame's corner is scrolled away). It is hosted in its own window ([PinnedChromeWindow]), because the
 * ink surface is above this window while frames exist (contract §3.5).
 */
@Composable
internal fun FrameChips(
    specs: List<FrameSpec>,
    locked: String?,
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
    // The Ghost toggles: a local before/after view per frame (default on), never synced, gone with the page.
    var ghosts by remember { mutableStateOf<Map<String, Boolean>>(emptyMap()) }
    val actions =
        FrameChipActions(
            onPreset = onPreset,
            onRoute = onRoute,
            onGhost = { id, on ->
                ghosts = ghosts + (id to on)
                layer.setPatchOn(id, on)
            },
        )
    // While the camera moves the chip's window stays away: a window that moved with every camera frame would be
    // repositioned by the window manager on each one. It comes back when the camera has rested.
    var resting by remember { mutableStateOf(true) }
    LaunchedEffect(transform) {
        resting = false
        delay(REST_MS)
        resting = true
    }
    val density = LocalDensity.current.density
    // The route dialog belongs here, not to the chip: the chip's window leaves the composition while the camera moves.
    var editingRoute by remember { mutableStateOf<String?>(null) }
    val spec = specs.firstOrNull { it.id == locked }
    if (spec != null && resting) {
        val box = FrameRect(spec.x, spec.y, spec.cssWidth.toFloat(), spec.boxHeight.toFloat())
        val placement = FrameGeometry.place(box, transform, density)
        PinnedChromeWindow(placement.left.roundToInt() + CHIP_INSET_PX, placement.top.roundToInt() + CHIP_INSET_PX) {
            key(spec.id) {
                FrameChip(spec, FrameChipState.of(spec, ghosts[spec.id] ?: true), layer, actions, onEditRoute = {
                    editingRoute = spec.id
                })
            }
        }
    }
    specs.firstOrNull { it.id == editingRoute }?.let { editing ->
        RouteDialog(
            current = editing.route,
            onDismiss = { editingRoute = null },
            onOpen = { route ->
                editingRoute = null
                actions.onRoute(editing.id, route)
            },
        )
    }
}

/** One frame's controls: preset (tap cycles), reload, route (tap to open another) and Ghost. */
@Composable
private fun FrameChip(
    spec: FrameSpec,
    state: FrameChipState,
    layer: FrameLayer,
    actions: FrameChipActions,
    onEditRoute: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Column(
        modifier
            .background(Color(CHIP_BACKGROUND), RoundedCornerShape(CHIP_RADIUS_DP.dp))
            .padding(CHIP_PAD_DP.dp),
        verticalArrangement = Arrangement.spacedBy(CHIP_GAP_DP.dp),
    ) {
        Row(
            horizontalArrangement = Arrangement.spacedBy(CHIP_GAP_DP.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            ToolChip(
                label = spec.preset,
                selected = false,
                onClick = { actions.onPreset(spec.id, FramePreset.nextFrom(spec.preset).key) },
            )
            ToolButton(
                icon = ToolIcons.Sync,
                label = stringResource(R.string.frame_reload),
                selected = false,
                onClick = { layer.reload(spec.id) },
            )
            ToolChip(label = spec.route, selected = false, onClick = onEditRoute)
            if (state.ghost) {
                ToolChip(
                    label = stringResource(R.string.frame_ghost),
                    selected = state.ghostOn,
                    onClick = { actions.onGhost(spec.id, !state.ghostOn) },
                )
            }
        }
    }
}
