package dev.penwise.frames

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

private const val CHIP_HEIGHT_DP = 40
private const val CHIP_TEXT_SP = 13

/** A pill in the graphite chrome. [primary] is the one action of the screen (Send): filled, with the dark label. */
@Composable
internal fun ChromeChip(
    label: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    primary: Boolean = false,
) {
    val colors = ChromeStyle.colors
    val shape = RoundedCornerShape((CHIP_HEIGHT_DP / 2).dp)
    Box(
        modifier
            .height(CHIP_HEIGHT_DP.dp)
            .clip(shape)
            .background(if (primary) colors.accent else colors.panel)
            .then(if (primary) Modifier else Modifier.border(ChromeStyle.HAIRLINE_DP.dp, colors.hairline, shape))
            .clickable(role = Role.Button, onClick = onClick)
            .padding(horizontal = 16.dp),
        contentAlignment = Alignment.Center,
    ) {
        BasicText(
            label,
            style =
                TextStyle(
                    color = if (primary) colors.onAccent else colors.text,
                    fontSize = CHIP_TEXT_SP.sp,
                    fontWeight = if (primary) FontWeight.SemiBold else FontWeight.Normal,
                    fontFamily = ChromeStyle.font,
                ),
        )
    }
}
