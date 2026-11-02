package dev.penwise.tools

import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ColorFilter
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntRect
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.window.Popup
import androidx.compose.ui.window.PopupPositionProvider
import kotlinx.coroutines.delay

private const val ICON_TINT = 0xFF23252A
private const val SELECTED_TINT = 0xFF2F4FD0
private const val CHIP_BACKGROUND = 0x14000000
private const val SELECTED_BACKGROUND = 0x1A2F4FD0
private const val CHIP_PADDING_DP = 7
private const val ICON_SIZE_DP = 22
private const val TIP_BACKGROUND = 0xE6202124
private const val TIP_TEXT_SP = 13
private const val TIP_MS = 1_600L
private const val TIP_GAP_PX = 12

/**
 * A tappable icon with an optional selected state. [label] is the screen-reader name and the long-press tooltip.
 *
 */
@OptIn(ExperimentalFoundationApi::class)
@Composable
fun ToolButton(
    icon: ImageVector,
    label: String,
    selected: Boolean,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    var tip by remember { mutableStateOf(false) }
    val tint = Color(if (selected) SELECTED_TINT else ICON_TINT)
    Box(modifier) {
        Column(
            modifier =
                Modifier
                    .clip(RoundedCornerShape(8.dp))
                    .background(Color(if (selected) SELECTED_BACKGROUND else CHIP_BACKGROUND))
                    .combinedClickable(role = Role.Button, onLongClick = { tip = true }, onClick = onClick)
                    .padding(CHIP_PADDING_DP.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Image(
                imageVector = icon,
                contentDescription = label,
                colorFilter = ColorFilter.tint(tint),
                modifier = Modifier.size(ICON_SIZE_DP.dp),
            )
        }
        if (tip) Tooltip(label) { tip = false }
    }
}

/** Plain-text tooltip under the anchor (above it when there is no room below) that goes away on its own. */
@Composable
internal fun Tooltip(
    text: String,
    onDismiss: () -> Unit,
) {
    LaunchedEffect(text) {
        delay(TIP_MS)
        onDismiss()
    }
    Popup(
        popupPositionProvider =
            object : PopupPositionProvider {
                override fun calculatePosition(
                    anchorBounds: IntRect,
                    windowSize: IntSize,
                    layoutDirection: LayoutDirection,
                    popupContentSize: IntSize,
                ): IntOffset {
                    val x =
                        (anchorBounds.center.x - popupContentSize.width / 2).coerceIn(
                            0,
                            windowSize.width - popupContentSize.width,
                        )
                    val below = anchorBounds.bottom + TIP_GAP_PX
                    val fits = below + popupContentSize.height <= windowSize.height
                    return IntOffset(x, if (fits) below else anchorBounds.top - TIP_GAP_PX - popupContentSize.height)
                }
            },
        onDismissRequest = onDismiss,
    ) {
        Box(
            Modifier
                .background(
                    Color(TIP_BACKGROUND),
                    RoundedCornerShape(6.dp),
                ).padding(horizontal = 10.dp, vertical = 6.dp),
        ) {
            BasicText(text, style = TextStyle(color = Color.White, fontSize = TIP_TEXT_SP.sp))
        }
    }
}
