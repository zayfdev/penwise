package dev.penwise.tools

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.window.Dialog
import dev.penwise.R

private const val FIELD_WIDTH_DP = 320
private const val PANEL_RADIUS_DP = 12
private const val FIELD_RADIUS_DP = 8
private const val GAP_DP = 10
private const val LABEL_SIZE_SP = 12
private const val TEXT_SIZE_SP = 14
private const val PAD_H_DP = 10
private const val PAD_V_DP = 10
private const val SMALL_GAP_DP = 4
private const val ROW_GAP_DP = 8
private const val PANEL_COLOR = 0xFF23252A.toInt()
private const val FIELD_COLOR = 0xFF16181C.toInt()
private const val LABEL_COLOR = 0xFFB9BEC7.toInt()
private const val PANEL_PAD_DP = 16
private const val FIELD_HEIGHT_DP = 40

/**
 * PC server address and access token, with the link state on top. Saving reconnects immediately; nothing is
 * validated here because a wrong address is not an error the user needs a dialog about.
 */
@Composable
fun SyncDialog(
    initialUrl: String,
    initialToken: String,
    status: String,
    onDismiss: () -> Unit,
    onSave: (url: String, token: String) -> Unit,
) {
    var url by remember { mutableStateOf(initialUrl) }
    var token by remember { mutableStateOf(initialToken) }
    Dialog(onDismissRequest = onDismiss) {
        Column(
            Modifier
                .background(Color(PANEL_COLOR), RoundedCornerShape(PANEL_RADIUS_DP.dp))
                .padding(PANEL_PAD_DP.dp),
            verticalArrangement = Arrangement.spacedBy(GAP_DP.dp),
        ) {
            androidx.compose.foundation.text.BasicText(
                status,
                style = TextStyle(fontSize = TEXT_SIZE_SP.sp, color = Color(LABEL_COLOR)),
            )
            Field(stringResource(R.string.sync_url_label), url) { url = it }
            Field(stringResource(R.string.sync_token_label), token) { token = it }
            Row(horizontalArrangement = Arrangement.spacedBy(ROW_GAP_DP.dp)) {
                ToolChip(stringResource(R.string.cancel), selected = false, onClick = onDismiss)
                ToolChip(stringResource(R.string.save), selected = true, onClick = { onSave(url.trim(), token.trim()) })
            }
        }
    }
}

@Composable
private fun Field(
    label: String,
    value: String,
    onChange: (String) -> Unit,
) {
    Column(verticalArrangement = Arrangement.spacedBy(SMALL_GAP_DP.dp)) {
        androidx.compose.foundation.text.BasicText(
            label,
            style = TextStyle(fontSize = LABEL_SIZE_SP.sp, color = Color(LABEL_COLOR)),
        )
        BasicTextField(
            value = value,
            onValueChange = onChange,
            singleLine = true,
            textStyle = TextStyle(fontSize = TEXT_SIZE_SP.sp, color = Color.White),
            modifier =
                Modifier
                    .width(FIELD_WIDTH_DP.dp)
                    .background(Color(FIELD_COLOR), RoundedCornerShape(FIELD_RADIUS_DP.dp))
                    .padding(horizontal = PAD_H_DP.dp, vertical = PAD_V_DP.dp)
                    .fillMaxWidth(),
        )
    }
}
