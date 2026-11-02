package dev.penwise.tools

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.penwise.frames.ChromeStyle

private const val CHIP_RADIUS_DP = 8
private const val CHIP_PAD_H_DP = 10
private const val CHIP_PAD_V_DP = 6
private const val CHIP_TEXT_SP = 13
private const val SELECTED_DARK = 0xFF202124
private const val CHIP_LIGHT = 0xFFECECEC

/** Text button; [selected] renders it inverted. */
@Composable
internal fun ToolChip(
    label: String,
    selected: Boolean,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val shape = RoundedCornerShape(CHIP_RADIUS_DP.dp)
    Box(
        modifier
            .clip(shape)
            .background(if (selected) Color(SELECTED_DARK) else Color(CHIP_LIGHT))
            .clickable(role = Role.Button, onClick = onClick)
            .padding(horizontal = CHIP_PAD_H_DP.dp, vertical = CHIP_PAD_V_DP.dp),
        contentAlignment = Alignment.Center,
    ) {
        BasicText(
            text = label,
            style =
                TextStyle(
                    fontFamily = ChromeStyle.font,
                    color = if (selected) Color.White else Color.Black,
                    fontSize = CHIP_TEXT_SP.sp,
                ),
        )
    }
}
