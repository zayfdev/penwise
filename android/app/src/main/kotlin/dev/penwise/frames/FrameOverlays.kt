package dev.penwise.frames

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import dev.penwise.R
import dev.penwise.tools.ToolChip

private const val MARGIN_DP = 12
private const val GAP_DP = 8
private const val DISABLED_ALPHA = 0.4f
private const val DISMISS = "\u00D7"

/** What the frames' chip area shows besides the frames' own controls, and the calls behind it. */
internal class FrameOverlayModel(
    /** The page has frames, so there is a round to send. */
    val showSend: Boolean,
    /** The Send button is enabled: the tablet is online. */
    val canSend: Boolean,
    /** The held camera move of another device, or null. */
    val chip: MoveTarget?,
    val onSend: () -> Unit,
    val onChipTap: () -> Unit,
    val onChipDismiss: () -> Unit,
)

/**
 * The chip area at the top edge of the stage (the UI design pass is deferred; this is the minimal, working form): the
 * Send button, which closes the open round (spec §4) and is disabled while offline, and the chip
 * "Agent: open <label> →" that does a camera move held back because the owner was drawing.
 */
@Composable
internal fun FrameOverlays(
    model: FrameOverlayModel,
    modifier: Modifier = Modifier,
) {
    Box(modifier.fillMaxSize().safeDrawingPadding().padding(MARGIN_DP.dp)) {
        Row(
            modifier = Modifier.align(Alignment.TopEnd),
            horizontalArrangement = Arrangement.spacedBy(GAP_DP.dp),
        ) {
            model.chip?.let { target ->
                ToolChip(
                    label = stringResource(R.string.agent_open, target.label),
                    selected = true,
                    onClick = model.onChipTap,
                )
                ToolChip(label = DISMISS, selected = false, onClick = model.onChipDismiss)
            }
            if (model.showSend) {
                ToolChip(
                    label = stringResource(R.string.round_send),
                    selected = false,
                    onClick = if (model.canSend) model.onSend else ({}),
                    modifier = if (model.canSend) Modifier else Modifier.alpha(DISABLED_ALPHA),
                )
            }
        }
    }
}
