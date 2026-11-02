package dev.penwise.tools

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import dev.penwise.R
import dev.penwise.frames.ChromeStyle
import dev.penwise.sync.SyncStatus

private const val DOT_DP = 10
private const val COUNT_SP = 12
private const val SYNC_WIDTH_DP = 56

/**
 * Link state at a glance, in the dock: green synced, amber connecting, red offline, with the edits still waiting to
 * send. Its description is the status text ("Synced", ...), which the device checks read; a tap opens the sync dialog.
 */
@Composable
internal fun SyncDot(
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
    val colors = ChromeStyle.colors
    val color =
        when (status) {
            SyncStatus.ONLINE -> colors.online
            SyncStatus.CONNECTING -> colors.connecting
            SyncStatus.BAD_TOKEN, SyncStatus.STOPPED -> colors.offline
        }
    // A fixed width, so the count appearing never moves the buttons beside it.
    DockSlot(description, selected = false, onClick = onClick, width = SYNC_WIDTH_DP.dp) {
        Row(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.CenterVertically) {
            Box(Modifier.size(DOT_DP.dp).background(color, CircleShape))
            if (pending > 0) DockText(pending.toString(), colors.text, COUNT_SP)
        }
    }
}
