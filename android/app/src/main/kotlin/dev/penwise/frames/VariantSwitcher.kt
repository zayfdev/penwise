package dev.penwise.frames

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.penwise.R

private const val BAR_PAD_DP = 6
private const val GAP_DP = 4
private const val BUTTON_PAD_H_DP = 12
private const val BUTTON_PAD_V_DP = 8
private const val TEXT_SP = 14
private const val CHECK = " ✓"

/**
 * The floating `OG · A · B · C` row of a lane that has variants (locked-frames contract §3.2), in the graphite chrome
 * ([ChromeStyle]). The shown item is filled, the chosen variant carries a check, a tap on an item calls [onSelect]
 * with its frame id, and "Use this" picks the shown variant. Nothing is drawn for a null [state]. The state is the
 * plain [SwitcherState]; this only draws it.
 */
@Composable
internal fun VariantSwitcher(
    state: SwitcherState?,
    onSelect: (id: String) -> Unit,
    onUseThis: (id: String) -> Unit,
    modifier: Modifier = Modifier,
) {
    if (state == null) return
    val panel = RoundedCornerShape(ChromeStyle.PANEL_RADIUS_DP.dp)
    Row(
        modifier
            .background(ChromeStyle.panel, panel)
            .border(ChromeStyle.HAIRLINE_DP.dp, ChromeStyle.hairline, panel)
            .padding(BAR_PAD_DP.dp),
        horizontalArrangement = Arrangement.spacedBy(GAP_DP.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        for (item in state.items) {
            SwitcherButton(
                label = if (item.chosen) item.label + CHECK else item.label,
                selected = item.id == state.shown,
                onClick = { onSelect(item.id) },
            )
        }
        if (state.canUseThis) {
            SwitcherButton(
                label = stringResource(R.string.frame_use_this),
                selected = false,
                outlined = true,
                onClick = { onUseThis(state.shown) },
            )
        }
    }
}

@Composable
private fun SwitcherButton(
    label: String,
    selected: Boolean,
    onClick: () -> Unit,
    outlined: Boolean = false,
) {
    val shape = RoundedCornerShape(ChromeStyle.BUTTON_RADIUS_DP.dp)
    Box(
        Modifier
            .clip(shape)
            .background(if (selected) ChromeStyle.buttonSelected else ChromeStyle.panel)
            .then(if (outlined) Modifier.border(ChromeStyle.HAIRLINE_DP.dp, ChromeStyle.hairline, shape) else Modifier)
            .clickable(role = Role.Button, onClick = onClick)
            .padding(horizontal = BUTTON_PAD_H_DP.dp, vertical = BUTTON_PAD_V_DP.dp),
        contentAlignment = Alignment.Center,
    ) {
        BasicText(
            text = label,
            style =
                TextStyle(
                    color = if (selected) ChromeStyle.text else ChromeStyle.textMuted,
                    fontSize = TEXT_SP.sp,
                    fontFamily = ChromeStyle.font,
                ),
        )
    }
}
