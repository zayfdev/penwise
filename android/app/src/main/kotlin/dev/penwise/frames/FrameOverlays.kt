package dev.penwise.frames

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import dev.penwise.R
import dev.penwise.tools.ToolChip

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
 * The chip area at the top-right of the screen, in its own window (the UI design pass is deferred; this is the
 * minimal, working form): the Send button, which closes the open round (spec §4) and is disabled while offline,
 * and the chip
 * "Agent: open <label> →" that does a camera move held back because the owner was drawing.
 */
@Composable
internal fun FrameOverlays(
    model: FrameOverlayModel,
    modifier: Modifier = Modifier,
) {
    if (model.chip == null && !model.showSend) return
    // Its own window: the ink surface is above the activity window while frames exist (contract §3.5).
    ChromeWindow(ChromeCorner.TOP_END) {
        Row(
            modifier = modifier,
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
