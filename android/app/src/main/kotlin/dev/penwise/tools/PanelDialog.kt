package dev.penwise.tools

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.window.Dialog

private const val PANEL = 0xFF23252A.toInt()
private const val PANEL_TEXT = 0xFFE8EAEE.toInt()
private const val PANEL_MUTED = 0xFF9AA0AA.toInt()
private const val PANEL_BUTTON = 0x22FFFFFF
private const val ACCENT = 0xFF2F4FD0.toInt()
private const val DANGER = 0xFFC62828.toInt()
private const val MAX_WIDTH_DP = 420

/** Small dark dialog: a question, a short explanation, a confirm and a Cancel button. */
@Composable
fun ConfirmDialog(
    title: String,
    message: String,
    confirm: String,
    onDismiss: () -> Unit,
    onConfirm: () -> Unit,
    destructive: Boolean = false,
) {
    Dialog(onDismissRequest = onDismiss) {
        Column(
            Modifier
                .widthIn(max = MAX_WIDTH_DP.dp)
                .background(Color(PANEL), RoundedCornerShape(12.dp))
                .padding(18.dp),
            verticalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            BasicText(title, style = TextStyle(fontSize = 16.sp, color = Color(PANEL_TEXT)))
            BasicText(message, style = TextStyle(fontSize = 13.sp, color = Color(PANEL_MUTED)))
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                PanelButton(confirm, primary = true, danger = destructive, onClick = onConfirm)
                PanelButton("Cancel", primary = false, onClick = onDismiss)
            }
        }
    }
}

/** Dialog button: filled accent (or red when [danger]) for the main action, translucent otherwise. */
@Composable
fun PanelButton(
    label: String,
    primary: Boolean,
    danger: Boolean = false,
    onClick: () -> Unit,
) {
    val fill =
        when {
            primary && danger -> Color(DANGER)
            primary -> Color(ACCENT)
            else -> Color(PANEL_BUTTON)
        }
    Box(
        Modifier
            .background(fill, RoundedCornerShape(8.dp))
            .clickable(onClick = onClick)
            .padding(horizontal = 14.dp, vertical = 8.dp),
    ) {
        BasicText(
            label,
            style = TextStyle(fontSize = 14.sp, color = if (primary) Color.White else Color(PANEL_TEXT)),
        )
    }
}
