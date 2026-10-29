package dev.penwise.tools

import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicText
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.ExpandLess
import androidx.compose.material.icons.filled.ExpandMore
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ColorFilter
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.penwise.R
import dev.penwise.sync.SyncStatus

private const val BAR_COLOR = 0xF2FFFFFF
private const val BAR_RADIUS_DP = 22
private const val BAR_PAD_DP = 4
private const val BAR_GAP_DP = 4
private const val SWATCH_DP = 32
private const val RING_DP = 2
private const val RING_COLOR = 0xFF202124
private const val LABEL_SP = 13
private const val LABEL_COLOR = 0xFF23252A
private const val DOT_DP = 12
private const val DOT_PAD_DP = 8
private const val DOT_ONLINE = 0xFF2E9E5B
private const val DOT_CONNECTING = 0xFFE0A100
private const val DOT_OFFLINE = 0xFFC0392B

/**
 * The only chrome on a collapsed page: current colour (opens the drawer), undo, redo and the "Tools" expander.
 * Small on purpose; it is the single thing covering the page.
 */
@Composable
fun MiniBar(
    state: ToolState,
    expanded: Boolean,
    onToggle: () -> Unit,
    onUndo: () -> Unit,
    onRedo: () -> Unit,
    syncStatus: SyncStatus,
    pending: Int,
    onSync: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Row(
        modifier
            .background(Color(BAR_COLOR), RoundedCornerShape(BAR_RADIUS_DP.dp))
            .padding(BAR_PAD_DP.dp),
        horizontalArrangement = Arrangement.spacedBy(BAR_GAP_DP.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        CurrentColor(state, onClick = onToggle)
        ToolChip(
            label =
                stringResource(
                    if (state.bindToElement) R.string.frame_bind_element else R.string.frame_bind_area,
                ),
            selected = state.bindToElement,
            onClick = state::toggleBindTarget,
        )
        ToolButton(ToolIcons.Undo, stringResource(R.string.tool_undo), false, onUndo)
        ToolButton(ToolIcons.Redo, stringResource(R.string.tool_redo), false, onRedo)
        SyncDot(syncStatus, pending, onSync)
        Expander(expanded, onToggle)
    }
}

/** Link state at a glance: green synced, amber connecting, red offline, with the edits still waiting to send. */
@Composable
private fun SyncDot(
    status: SyncStatus,
    pending: Int,
    onClick: () -> Unit,
) {
    val label =
        stringResource(
            when (status) {
                SyncStatus.ONLINE -> R.string.sync_dot_online
                SyncStatus.CONNECTING -> R.string.sync_dot_connecting
                SyncStatus.BAD_TOKEN, SyncStatus.STOPPED -> R.string.sync_dot_offline
            },
        )
    val description = if (pending > 0) stringResource(R.string.sync_dot_pending, label, pending) else label
    val color =
        when (status) {
            SyncStatus.ONLINE -> DOT_ONLINE
            SyncStatus.CONNECTING -> DOT_CONNECTING
            SyncStatus.BAD_TOKEN, SyncStatus.STOPPED -> DOT_OFFLINE
        }
    Row(
        Modifier
            .clip(RoundedCornerShape(BAR_RADIUS_DP.dp))
            .combinedClickable(role = Role.Button, onClick = onClick)
            .semantics { contentDescription = description }
            .padding(horizontal = DOT_PAD_DP.dp, vertical = DOT_PAD_DP.dp),
        horizontalArrangement = Arrangement.spacedBy(BAR_GAP_DP.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(Modifier.size(DOT_DP.dp).background(Color(color), CircleShape))
        if (pending > 0) {
            BasicText(pending.toString(), style = TextStyle(color = Color(LABEL_COLOR), fontSize = LABEL_SP.sp))
        }
    }
}

/** Swatch of the colour the pen will write with; tapping it opens the drawer. */
@Composable
private fun CurrentColor(
    state: ToolState,
    onClick: () -> Unit,
) {
    val description = stringResource(R.string.tool_tools_open)
    var tip by remember { mutableStateOf(false) }
    Box(
        Modifier
            .size(SWATCH_DP.dp)
            .semantics { contentDescription = description }
            .clip(CircleShape)
            .background(Color(state.argb() or OPAQUE))
            .border(RING_DP.dp, Color(RING_COLOR), CircleShape)
            .combinedClickable(role = Role.Button, onLongClick = { tip = true }, onClick = onClick),
    ) {
        if (tip) Tooltip(description) { tip = false }
    }
}

@Composable
private fun Expander(
    expanded: Boolean,
    onToggle: () -> Unit,
) {
    val label = stringResource(if (expanded) R.string.tool_tools_close else R.string.tool_tools_open)
    Row(
        Modifier
            .clip(RoundedCornerShape(BAR_RADIUS_DP.dp))
            .background(Color(CHIP_FILL))
            .combinedClickable(role = Role.Button, onClick = onToggle)
            .semantics { contentDescription = label }
            .padding(start = 10.dp, end = 4.dp, top = 6.dp, bottom = 6.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        BasicText(
            stringResource(R.string.tool_tools),
            style = TextStyle(color = Color(LABEL_COLOR), fontSize = LABEL_SP.sp),
        )
        Image(
            imageVector = if (expanded) Icons.Filled.ExpandLess else Icons.Filled.ExpandMore,
            contentDescription = null,
            colorFilter = ColorFilter.tint(Color(LABEL_COLOR)),
            modifier = Modifier.size(20.dp),
        )
    }
}

private const val OPAQUE = -0x1000000
private const val CHIP_FILL = 0x14000000
