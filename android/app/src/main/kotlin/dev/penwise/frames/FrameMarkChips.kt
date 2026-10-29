package dev.penwise.frames

import androidx.annotation.StringRes
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.CircleShape
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
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Popup
import dev.penwise.R
import dev.penwise.ink.ViewTransform
import dev.penwise.ink.ViewportState
import dev.penwise.tools.ToolChip
import kotlin.math.roundToInt

private const val CHIP_DP = 22
private const val RING_DP = 2
private const val MENU_PAD_DP = 6
private const val MENU_GAP_DP = 4
private const val MENU_MAX_WIDTH_DP = 220
private const val DOT_NEW = 0xFF2F4FD0
private const val DOT_SEEN = 0xFF9AA0AA
private const val DOT_DONE = 0xFF2E9E5B
private const val DOT_REOPENED = 0xFFE08A1E
private const val MENU_BACKGROUND = 0xF2FFFFFF
private const val MENU_RADIUS_DP = 10

/**
 * A chip at each visible mark's rect corner (K3): blue = new, grey = seen, orange = reopened, green = done (shown only
 * while "show done" is on, which the core enforces by dropping the rect). Tapping opens Done / Reopen and, when the
 * mark's state recipe is not the page's current one, Replay state.
 */
@Composable
internal fun FrameMarkChips(
    marks: List<SceneMark>,
    specs: List<FrameSpec>,
    viewport: ViewportState,
    layer: FrameLayer,
    states: Map<String, String>,
    onStatus: (id: String, status: String) -> Unit,
) {
    if (marks.isEmpty()) return
    var transform by remember { mutableStateOf(viewport.transform) }
    DisposableEffect(viewport) {
        val observer: (ViewTransform) -> Unit = { transform = it }
        viewport.observers.add(observer)
        onDispose { viewport.observers.remove(observer) }
    }
    Box(Modifier.fillMaxSize()) {
        for (chip in MarkPlacement.chips(marks, specs, states, transform)) {
            key(chip.id) {
                MarkChipView(
                    chip = chip,
                    layer = layer,
                    onStatus = onStatus,
                    modifier =
                        Modifier.offset {
                            IntOffset(chip.left.roundToInt(), chip.top.roundToInt())
                        },
                )
            }
        }
    }
}

@Composable
private fun MarkChipView(
    chip: MarkChip,
    layer: FrameLayer,
    onStatus: (id: String, status: String) -> Unit,
    modifier: Modifier = Modifier,
) {
    var open by remember { mutableStateOf(false) }
    val description = stringResource(statusLabel(chip.status))
    Box(modifier) {
        Box(
            Modifier
                .size(CHIP_DP.dp)
                .semantics { contentDescription = description }
                .clip(CircleShape)
                .background(Color(statusColor(chip.status)))
                .border(RING_DP.dp, Color.White, CircleShape)
                .clickable(role = Role.Button) { open = true },
        )
        if (open) {
            MarkMenu(
                chip = chip,
                onDismiss = { open = false },
                onStatus = onStatus,
                onReplay = { layer.replay(chip.frame, chip.state) },
            )
        }
    }
}

@Composable
private fun MarkMenu(
    chip: MarkChip,
    onDismiss: () -> Unit,
    onStatus: (id: String, status: String) -> Unit,
    onReplay: () -> Unit,
) {
    val gap = with(LocalDensity.current) { (CHIP_DP + MENU_GAP_DP).dp.roundToPx() }
    Popup(
        alignment = Alignment.TopStart,
        offset = IntOffset(0, gap),
        onDismissRequest = onDismiss,
    ) {
        Column(
            Modifier
                .widthIn(max = MENU_MAX_WIDTH_DP.dp)
                .background(Color(MENU_BACKGROUND), RoundedCornerShape(MENU_RADIUS_DP.dp))
                .padding(MENU_PAD_DP.dp),
            verticalArrangement = Arrangement.spacedBy(MENU_GAP_DP.dp),
        ) {
            ToolChip(
                label = stringResource(R.string.mark_action_done),
                selected = false,
                onClick = {
                    onDismiss()
                    onStatus(chip.id, MarkStatus.DONE.wire)
                },
            )
            ToolChip(
                label = stringResource(R.string.mark_action_reopen),
                selected = false,
                onClick = {
                    onDismiss()
                    onStatus(chip.id, MarkStatus.REOPENED.wire)
                },
            )
            if (chip.dimmed) {
                ToolChip(
                    label = stringResource(R.string.mark_action_replay),
                    selected = false,
                    onClick = {
                        onDismiss()
                        onReplay()
                    },
                )
            }
        }
    }
}

private fun statusColor(status: MarkStatus): Long =
    when (status) {
        MarkStatus.NEW -> DOT_NEW
        MarkStatus.SEEN -> DOT_SEEN
        MarkStatus.DONE -> DOT_DONE
        MarkStatus.REOPENED -> DOT_REOPENED
    }

@StringRes
private fun statusLabel(status: MarkStatus): Int =
    when (status) {
        MarkStatus.NEW -> R.string.mark_status_new
        MarkStatus.SEEN -> R.string.mark_status_seen
        MarkStatus.DONE -> R.string.mark_status_done
        MarkStatus.REOPENED -> R.string.mark_status_reopened
    }
