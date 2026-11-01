package dev.penwise.overview

import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.GridView
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.ColorFilter
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import dev.penwise.R
import dev.penwise.frames.ChromeCorner
import dev.penwise.frames.ChromeStyle
import dev.penwise.frames.ChromeWindow

private const val ICON_DP = 24
private const val PAD_DP = 10

/** The overview button (contract §3.3), in its own window above the ink at the screen's bottom-left. */
@Composable
internal fun OverviewButton(
    visible: Boolean,
    onClick: () -> Unit,
) {
    if (!visible) return
    val shape = RoundedCornerShape(ChromeStyle.BUTTON_RADIUS_DP.dp)
    val label = stringResource(R.string.overview_open)
    ChromeWindow(ChromeCorner.BOTTOM_START) {
        Image(
            imageVector = Icons.Filled.GridView,
            contentDescription = null,
            colorFilter = ColorFilter.tint(ChromeStyle.text),
            modifier =
                Modifier
                    .clip(shape)
                    .background(ChromeStyle.panel)
                    .border(ChromeStyle.HAIRLINE_DP.dp, ChromeStyle.hairline, shape)
                    .semantics { contentDescription = label }
                    .clickable(role = Role.Button, onClick = onClick)
                    .padding(PAD_DP.dp)
                    .size(ICON_DP.dp),
        )
    }
}
