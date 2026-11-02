package dev.penwise.home

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicText
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.window.Dialog
import dev.penwise.frames.ChromeStyle
import dev.penwise.ink.PageBook
import dev.penwise.ink.PageList
import dev.penwise.ink.PageMeta
import dev.penwise.ink.PageTemplate
import dev.penwise.tools.PanelButton
import dev.penwise.tools.ToolButton
import dev.penwise.tools.ToolIcons

@Composable
internal fun TextButtonRow(
    label: String,
    onClick: () -> Unit,
) {
    Box(
        Modifier
            .background(Color(ACCENT_FILL), RoundedCornerShape(RADIUS_DP.dp))
            .clickable(onClick = onClick)
            .padding(horizontal = 16.dp, vertical = 10.dp),
    ) {
        BasicText(label, style = TextStyle(fontFamily = ChromeStyle.font, fontSize = 14.sp, color = Color(ACCENT)))
    }
}

/** Small dark dialog with one field and up to three actions. */
@Composable
internal fun PromptDialog(
    title: String,
    label: String,
    initial: String,
    confirm: String,
    onDismiss: () -> Unit,
    onConfirm: (String) -> Unit,
    extra: Pair<String, () -> Unit>? = null,
) {
    var text by remember { mutableStateOf(initial) }
    Dialog(onDismissRequest = onDismiss) {
        Column(
            Modifier
                .background(Color(PANEL), RoundedCornerShape(12.dp))
                .padding(18.dp),
            verticalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            BasicText(
                title,
                style = TextStyle(fontFamily = ChromeStyle.font, fontSize = 16.sp, color = Color(PANEL_TEXT)),
            )
            BasicText(
                label,
                style = TextStyle(fontFamily = ChromeStyle.font, fontSize = 11.sp, color = Color(PANEL_MUTED)),
            )
            BasicTextField(
                value = text,
                onValueChange = { text = it },
                singleLine = true,
                textStyle = TextStyle(fontFamily = ChromeStyle.font, fontSize = 15.sp, color = Color.White),
                modifier =
                    Modifier
                        .fillMaxWidth()
                        .background(Color(FIELD), RoundedCornerShape(8.dp))
                        .padding(horizontal = 10.dp, vertical = 9.dp),
            )
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                PanelButton(confirm, primary = true) { onConfirm(text.trim()) }
                PanelButton("Cancel", primary = false, onClick = onDismiss)
                extra?.let { (label, action) ->
                    PanelButton(label, primary = false, onClick = action)
                }
            }
        }
    }
}

/** New page dialog: title plus the four templates. */
@Composable
fun NewPageDialog(
    suggested: String,
    onDismiss: () -> Unit,
    onCreate: (String, PageTemplate) -> Unit,
) {
    var title by remember { mutableStateOf("") }
    var template by remember { mutableStateOf(PageTemplate.Blank) }
    Dialog(onDismissRequest = onDismiss) {
        Column(
            Modifier
                .background(Color(PANEL), RoundedCornerShape(12.dp))
                .padding(18.dp),
            verticalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            BasicText(
                "New page",
                style = TextStyle(fontFamily = ChromeStyle.font, fontSize = 16.sp, color = Color(PANEL_TEXT)),
            )
            BasicTextField(
                value = title,
                onValueChange = { title = it },
                singleLine = true,
                textStyle = TextStyle(fontFamily = ChromeStyle.font, fontSize = 15.sp, color = Color.White),
                modifier =
                    Modifier
                        .fillMaxWidth()
                        .background(Color(FIELD), RoundedCornerShape(8.dp))
                        .padding(horizontal = 10.dp, vertical = 9.dp),
                decorationBox = { inner ->
                    if (title.isEmpty()) {
                        BasicText(
                            suggested,
                            style =
                                TextStyle(
                                    fontFamily = ChromeStyle.font,
                                    fontSize = 15.sp,
                                    color = Color(PANEL_MUTED),
                                ),
                        )
                    }
                    inner()
                },
            )
            PageTemplate.entries.forEach { option ->
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    modifier =
                        Modifier
                            .fillMaxWidth()
                            .clickable { template = option }
                            .padding(vertical = 2.dp),
                ) {
                    BasicText(
                        if (template == option) "●" else "○",
                        style = TextStyle(fontFamily = ChromeStyle.font, fontSize = 13.sp, color = Color(PANEL_TEXT)),
                    )
                    Spacer(Modifier.width(8.dp))
                    BasicText(
                        option.label,
                        style = TextStyle(fontFamily = ChromeStyle.font, fontSize = 14.sp, color = Color(PANEL_TEXT)),
                    )
                    Spacer(Modifier.width(8.dp))
                    BasicText(
                        option.hint,
                        style = TextStyle(fontFamily = ChromeStyle.font, fontSize = 11.sp, color = Color(PANEL_MUTED)),
                    )
                }
            }
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                PanelButton("Create", primary = true) { onCreate(title.ifBlank { suggested }, template) }
                PanelButton("Cancel", primary = false, onClick = onDismiss)
            }
        }
    }
}
