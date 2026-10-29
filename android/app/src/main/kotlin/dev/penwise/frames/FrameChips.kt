package dev.penwise.frames

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicText
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
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
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
private const val LABEL_SP = 12
private const val LABEL_COLOR = 0xFF37474F
private const val LABEL_PAD_DP = 4
private const val CHOSEN_COLOR = 0xFF1F5FD6
private const val CHOSEN_BORDER_DP = 2
private const val CHOSEN_RADIUS_DP = 4

/** The chip row's callbacks: the frame item edits and the choice the owner makes with a tap. */
private class FrameChipActions(
    val onPreset: (id: String, preset: String) -> Unit,
    val onRoute: (id: String, route: String) -> Unit,
    val onChoose: (id: String) -> Unit,
    val onGhost: (id: String, on: Boolean) -> Unit,
)

/**
 * The canvas controls of every frame, at the frame's top edge and outside the page: the preset switch, reload, the
 * route (which opens a small field), the Ghost toggle of a patched frame and, on a variant, its label and "Use this".
 * A chosen variant gets a highlighted border; the others are dimmed by [FrameLayer], so the pick reads at a glance.
 *
 * Screen-space: they keep their size while the canvas zooms, and they follow the frames through [viewport].
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
    onChoose: (id: String) -> Unit,
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
            onChoose = onChoose,
            onGhost = { id, on ->
                ghosts = ghosts + (id to on)
                layer.setPatchOn(id, on)
            },
        )
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
                val state = FrameChipState.of(spec, ghosts[spec.id] ?: true)
                if (state.chosen) ChosenBorder(placement)
                FrameChip(
                    spec = spec,
                    state = state,
                    layer = layer,
                    actions = actions,
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

/** The pick's highlight: a border around the chosen variant, on top of its page and outside the ink. */
@Composable
private fun ChosenBorder(placement: FramePlacement) {
    val density = LocalDensity.current.density
    Box(
        Modifier
            .offset { IntOffset(placement.left.roundToInt(), placement.top.roundToInt()) }
            .size((placement.width / density).dp, (placement.height / density).dp)
            .border(CHOSEN_BORDER_DP.dp, Color(CHOSEN_COLOR), RoundedCornerShape(CHOSEN_RADIUS_DP.dp)),
    )
}

/** One frame's controls: preset (tap cycles), reload, route (tap to open another), Ghost, variant label and pick. */
@Composable
private fun FrameChip(
    spec: FrameSpec,
    state: FrameChipState,
    layer: FrameLayer,
    actions: FrameChipActions,
    modifier: Modifier = Modifier,
) {
    var editingRoute by remember { mutableStateOf(false) }
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
            ToolChip(label = spec.route, selected = false, onClick = { editingRoute = true })
            if (state.ghost) {
                ToolChip(
                    label = stringResource(R.string.frame_ghost),
                    selected = state.ghostOn,
                    onClick = { actions.onGhost(spec.id, !state.ghostOn) },
                )
            }
        }
        // A variant's name and pick get their own row: the first row already nearly spans a phone frame.
        if (spec.group != null) {
            Row(
                horizontalArrangement = Arrangement.spacedBy(CHIP_GAP_DP.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                VariantLabel(state.label.ifEmpty { stringResource(R.string.frame_variant) })
                if (state.useThis) {
                    ToolChip(
                        label = stringResource(R.string.frame_use_this),
                        selected = true,
                        onClick = { actions.onChoose(spec.id) },
                    )
                }
            }
        }
    }
    if (editingRoute) {
        RouteDialog(
            current = spec.route,
            onDismiss = { editingRoute = false },
            onOpen = { route ->
                editingRoute = false
                actions.onRoute(spec.id, route)
            },
        )
    }
}

/** A variant's name, plain text between the controls: it is a label, not a button. */
@Composable
private fun VariantLabel(text: String) {
    BasicText(
        text = text,
        style = TextStyle(fontSize = LABEL_SP.sp, color = Color(LABEL_COLOR)),
        modifier = Modifier.padding(horizontal = LABEL_PAD_DP.dp),
    )
}
