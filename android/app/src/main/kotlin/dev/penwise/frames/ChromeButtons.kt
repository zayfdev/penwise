package dev.penwise.frames

import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.ColorFilter
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import dev.penwise.tools.DockText

private const val SMALL_HEIGHT_DP = 32
private const val SMALL_TEXT_SP = 13

/** A small text button in the graphite chrome (the frame chip's preset, route and Ghost); [selected] fills it. */
@Composable
internal fun ChromeTextButton(
    label: String,
    selected: Boolean,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val colors = ChromeStyle.colors
    Box(
        modifier
            .heightIn(min = SMALL_HEIGHT_DP.dp)
            .clip(RoundedCornerShape(ChromeStyle.BUTTON_RADIUS_DP.dp))
            .background(if (selected) colors.buttonSelected else colors.tileFill)
            .clickable(role = Role.Button, onClick = onClick)
            .padding(horizontal = 12.dp),
        contentAlignment = Alignment.Center,
    ) {
        DockText(label, if (selected) colors.text else colors.icon, SMALL_TEXT_SP)
    }
}

/** A small icon button in the graphite chrome. */
@Composable
internal fun ChromeIconButton(
    icon: ImageVector,
    label: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val colors = ChromeStyle.colors
    Box(
        modifier
            .widthIn(min = SMALL_HEIGHT_DP.dp)
            .heightIn(min = SMALL_HEIGHT_DP.dp)
            .clip(RoundedCornerShape(ChromeStyle.BUTTON_RADIUS_DP.dp))
            .background(colors.tileFill)
            .semantics { contentDescription = label }
            .clickable(role = Role.Button, onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        Image(icon, null, Modifier.size(icon.defaultWidth), colorFilter = ColorFilter.tint(colors.icon))
    }
}
