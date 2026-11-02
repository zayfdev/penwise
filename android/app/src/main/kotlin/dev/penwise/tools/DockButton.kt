package dev.penwise.tools

import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.ColorFilter
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import dev.penwise.frames.ChromeStyle

internal const val DOCK_BUTTON_DP = 48

/**
 * One dock control: a [DOCK_BUTTON_DP] tall rounded slot that fills when [selected], shrinks while pressed and shows
 * its [label] as a tooltip on a long press (also its screen-reader name). [content] is drawn centred.
 */
@OptIn(ExperimentalFoundationApi::class)
@Composable
internal fun DockSlot(
    label: String,
    selected: Boolean,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    width: Dp? = DOCK_BUTTON_DP.dp,
    content: @Composable BoxScope.() -> Unit,
) {
    val colors = ChromeStyle.colors
    val source = remember { MutableInteractionSource() }
    var tip by remember { mutableStateOf(false) }
    val fill by animateColorAsState(
        targetValue = if (selected) colors.buttonSelected else colors.buttonSelected.copy(alpha = 0f),
        animationSpec = tween(DockMotion.SELECT_MS, easing = DockMotion.easeOut),
        label = "dockFill",
    )
    Box(
        modifier
            .then(if (width == null) Modifier.widthIn(min = DOCK_BUTTON_DP.dp) else Modifier.width(width))
            .height(DOCK_BUTTON_DP.dp)
            .pressScale(source)
            .clip(RoundedCornerShape(ChromeStyle.BUTTON_RADIUS_DP.dp))
            .background(fill)
            .semantics { contentDescription = label }
            .combinedClickable(
                interactionSource = source,
                indication = null,
                role = Role.Button,
                onLongClick = { tip = true },
                onClick = onClick,
            ),
        contentAlignment = Alignment.Center,
    ) {
        content()
        if (tip) Tooltip(label) { tip = false }
    }
}

/** A dock control showing one [icon], tinted idle or selected. */
@Composable
internal fun DockButton(
    icon: ImageVector,
    label: String,
    selected: Boolean,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    width: Dp = DOCK_BUTTON_DP.dp,
) {
    DockSlot(label, selected, onClick, modifier, width) {
        val colors = ChromeStyle.colors
        val tint by animateColorAsState(
            targetValue = if (selected) colors.iconOn else colors.icon,
            animationSpec = tween(DockMotion.SELECT_MS, easing = DockMotion.easeOut),
            label = "dockTint",
        )
        Image(
            imageVector = icon,
            contentDescription = null,
            colorFilter = ColorFilter.tint(tint),
            modifier = Modifier.size(icon.defaultWidth),
        )
    }
}
