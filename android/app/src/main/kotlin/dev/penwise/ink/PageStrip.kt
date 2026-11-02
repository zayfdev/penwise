package dev.penwise.ink

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
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
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.window.Dialog
import dev.penwise.frames.ChromeStyle
import dev.penwise.tools.ToolChip

private const val CHIP_GAP_DP = 4
private const val CHIP_RADIUS_DP = 8
private const val MAX_CHIP_WIDTH_DP = 150
private const val TITLE_SIZE_SP = 13
private const val GLYPH_SIZE_SP = 13
private const val MUTED = 0xFF7A7A82.toInt()
private const val PANEL_COLOR = 0xFF23252A.toInt()
private const val PANEL_TEXT = 0xFFE8EAEE.toInt()
private const val PANEL_PAD_DP = 16
private const val FIELD_COLOR = 0xFF16181C.toInt()

/** Horizontal page chips with a `＋` that opens the new-page dialog. */
@Composable
fun PageStrip(
    pages: PageList,
    activeId: String,
    onSelect: (String) -> Unit,
    onNew: (String, PageTemplate) -> Unit,
    onRename: (String, String) -> Unit,
    onDelete: (String) -> Unit,
    modifier: Modifier = Modifier,
) {
    var creating by remember { mutableStateOf(false) }
    var editing by remember { mutableStateOf<PageMeta?>(null) }
    var deleting by remember { mutableStateOf<PageMeta?>(null) }

    val colors = ChromeStyle.colors
    val shape = RoundedCornerShape(ChromeStyle.PANEL_RADIUS_DP.dp)
    Row(
        modifier =
            modifier
                .background(colors.panel, shape)
                .border(ChromeStyle.HAIRLINE_DP.dp, colors.hairline, shape)
                .padding(CHIP_GAP_DP.dp)
                .horizontalScroll(rememberScrollState()),
        horizontalArrangement = Arrangement.spacedBy(CHIP_GAP_DP.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        pages.all().forEach { page ->
            PageChip(
                page = page,
                active = page.id == activeId,
                onClick = { onSelect(page.id) },
                onLongClick = { editing = page },
            )
        }
        Chip(
            label = "＋",
            glyph = false,
            active = false,
            onClick = { creating = true },
        )
    }

    if (creating) {
        NewPageDialog(
            suggested = pages.nextTitle(),
            onDismiss = { creating = false },
            onCreate = { title, template ->
                creating = false
                onNew(title, template)
            },
        )
    }

    editing?.let { page ->
        EditPageDialog(
            page = page,
            canDelete = pages.all().size > 1,
            onDismiss = { editing = null },
            onRename = { title ->
                editing = null
                onRename(page.id, title)
            },
            onDelete = {
                editing = null
                deleting = page
            },
        )
    }

    deleting?.let { page ->
        PanelDialog(onDismiss = { deleting = null }) {
            BasicText("\"${page.title}\"", style = TextStyle(fontSize = 15.sp, color = Color(PANEL_TEXT)))
            BasicText(
                "The page and its strokes are removed. This cannot be undone.",
                style = TextStyle(fontSize = 12.sp, color = Color(MUTED)),
            )
            DialogButtons {
                ToolChip("Delete", selected = true, onClick = {
                    deleting = null
                    onDelete(page.id)
                })
                ToolChip("Cancel", selected = false, onClick = { deleting = null })
            }
        }
    }
}

@Composable
private fun PageChip(
    page: PageMeta,
    active: Boolean,
    onClick: () -> Unit,
    onLongClick: () -> Unit,
) {
    val colors = ChromeStyle.colors
    Row(
        modifier =
            Modifier
                .widthIn(max = MAX_CHIP_WIDTH_DP.dp)
                .background(
                    if (active) colors.tileSelectedFill else colors.tileFill,
                    RoundedCornerShape(CHIP_RADIUS_DP.dp),
                ).combinedClickable(onClick = onClick, onLongClick = onLongClick)
                .padding(horizontal = 8.dp, vertical = 6.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        BasicText(
            page.glyph,
            style = TextStyle(fontSize = GLYPH_SIZE_SP.sp, color = colors.textMuted, fontFamily = ChromeStyle.font),
        )
        Spacer(Modifier.width(5.dp))
        BasicText(
            page.title,
            style =
                TextStyle(
                    fontSize = TITLE_SIZE_SP.sp,
                    color = colors.text,
                    fontFamily = ChromeStyle.font,
                ),
            maxLines = 1,
        )
        if (active) {
            Spacer(Modifier.width(5.dp))
            BasicText("•", style = TextStyle(fontSize = TITLE_SIZE_SP.sp, color = colors.text))
        }
    }
}

@Composable
private fun Chip(
    label: String,
    glyph: Boolean,
    active: Boolean,
    onClick: () -> Unit,
) {
    val colors = ChromeStyle.colors
    BasicText(
        label,
        style =
            TextStyle(
                fontSize = if (glyph) 16.sp else TITLE_SIZE_SP.sp,
                color = colors.text,
                fontFamily = ChromeStyle.font,
            ),
        modifier =
            Modifier
                .background(
                    if (active) colors.tileSelectedFill else colors.tileFill,
                    RoundedCornerShape(CHIP_RADIUS_DP.dp),
                ).clickable(onClick = onClick)
                .padding(horizontal = 8.dp, vertical = 6.dp),
    )
}

/** Title plus template picker for a new page. */
@Composable
private fun NewPageDialog(
    suggested: String,
    onDismiss: () -> Unit,
    onCreate: (String, PageTemplate) -> Unit,
) {
    var title by remember { mutableStateOf("") }
    var template by remember { mutableStateOf(PageTemplate.Blank) }
    PanelDialog(onDismiss = onDismiss) {
        BasicText("New page", style = TextStyle(fontSize = 15.sp, color = Color(PANEL_TEXT)))
        BasicTextField(
            value = title,
            onValueChange = { title = it },
            singleLine = true,
            textStyle = TextStyle(fontSize = 14.sp, color = Color.White),
            modifier =
                Modifier
                    .fillMaxWidth()
                    .background(Color(FIELD_COLOR), RoundedCornerShape(8.dp))
                    .padding(horizontal = 10.dp, vertical = 9.dp),
            decorationBox = { inner ->
                if (title.isEmpty()) {
                    BasicText(suggested, style = TextStyle(fontSize = 14.sp, color = Color(MUTED)))
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
                    style = TextStyle(fontSize = 13.sp, color = Color(PANEL_TEXT)),
                )
                Spacer(Modifier.width(8.dp))
                BasicText(option.label, style = TextStyle(fontSize = 14.sp, color = Color(PANEL_TEXT)))
                Spacer(Modifier.width(8.dp))
                BasicText(option.hint, style = TextStyle(fontSize = 11.sp, color = Color(MUTED)))
            }
        }
        DialogButtons {
            ToolChip("Create", selected = true, onClick = { onCreate(title.ifBlank { suggested }, template) })
            ToolChip("Cancel", selected = false, onClick = onDismiss)
        }
    }
}

/** Rename a page, with its delete affordance. */
@Composable
private fun EditPageDialog(
    page: PageMeta,
    canDelete: Boolean,
    onDismiss: () -> Unit,
    onRename: (String) -> Unit,
    onDelete: () -> Unit,
) {
    var title by remember { mutableStateOf(page.title) }
    PanelDialog(onDismiss = onDismiss) {
        BasicText("Page", style = TextStyle(fontSize = 15.sp, color = Color(PANEL_TEXT)))
        BasicTextField(
            value = title,
            onValueChange = { title = it },
            singleLine = true,
            textStyle = TextStyle(fontSize = 14.sp, color = Color.White),
            modifier =
                Modifier
                    .fillMaxWidth()
                    .background(Color(FIELD_COLOR), RoundedCornerShape(8.dp))
                    .padding(horizontal = 10.dp, vertical = 9.dp),
        )
        if (!canDelete) {
            BasicText(
                "The last page cannot be deleted.",
                style = TextStyle(fontSize = 11.sp, color = Color(MUTED)),
            )
        }
        DialogButtons {
            ToolChip("Rename", selected = true, onClick = { onRename(title) })
            if (canDelete) ToolChip("Delete", selected = false, onClick = onDelete)
            ToolChip("Cancel", selected = false, onClick = onDismiss)
        }
    }
}

/** Dark panel dialog frame, matching the sync dialog. */
@Composable
private fun PanelDialog(
    onDismiss: () -> Unit,
    content: @Composable () -> Unit,
) {
    Dialog(onDismissRequest = onDismiss) {
        Column(
            Modifier
                .background(Color(PANEL_COLOR), RoundedCornerShape(12.dp))
                .padding(PANEL_PAD_DP.dp),
            verticalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            content()
        }
    }
}

@Composable
private fun DialogButtons(content: @Composable RowScope.() -> Unit) {
    Row(
        horizontalArrangement = Arrangement.spacedBy(8.dp),
        content = content,
    )
}
