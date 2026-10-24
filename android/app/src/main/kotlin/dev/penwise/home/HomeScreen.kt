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
import dev.penwise.ink.PageBook
import dev.penwise.ink.PageList
import dev.penwise.ink.PageMeta
import dev.penwise.ink.PageTemplate
import dev.penwise.tools.Glyph
import dev.penwise.tools.GlyphChip

private const val BACKGROUND = 0xFFF4F5F7.toInt()
private const val CARD = 0xFFFFFFFF.toInt()
private const val INK = 0xFF1E2024.toInt()
private const val MUTED = 0xFF7C7F86.toInt()
private const val ACCENT = 0xFF2F4FD0.toInt()
private const val PANEL = 0xFF23252A.toInt()
private const val PANEL_TEXT = 0xFFE8EAEE.toInt()
private const val PANEL_MUTED = 0xFF9AA0AA.toInt()
private const val FIELD = 0xFF16181C.toInt()
private const val RADIUS_DP = 14
private const val SUBTLE_FILL = 0x0D000000
private const val ACCENT_FILL = 0x142F4FD0
private const val PANEL_BUTTON = 0x22FFFFFF
private const val PAD_DP = 20

/** Everything the home screen can ask the app to do. */
class HomeActions(
    val openPage: (String) -> Unit,
    val newPage: (String, PageTemplate, String?) -> Unit,
    val newNotebook: (String) -> Unit,
    val renameNotebook: (String, String) -> Unit,
    val deleteNotebook: (String) -> Unit,
    val renamePage: (String, String) -> Unit,
    val deletePage: (String) -> Unit,
    val sync: () -> Unit,
)

/**
 * Landing screen: notebooks, their pages, and the three things you do from here — new notebook, new page,
 * open a page. Naming follows the server: notebooks are titled, pages are titled and ordered.
 */
@Composable
fun HomeScreen(
    pages: PageList,
    actions: HomeActions,
    modifier: Modifier = Modifier,
) {
    var newNotebook by remember { mutableStateOf(false) }
    var newPageIn by remember { mutableStateOf<String?>(null) }
    var renamingBook by remember { mutableStateOf<PageBook?>(null) }
    var editingPage by remember { mutableStateOf<PageMeta?>(null) }

    Column(
        modifier
            .fillMaxSize()
            .background(Color(BACKGROUND))
            .safeDrawingPadding()
            .padding(PAD_DP.dp),
        verticalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        Header(
            pageCount = pages.all().size,
            notebookCount = pages.books.size,
            onNewNotebook = { newNotebook = true },
            onSync = actions.sync,
        )
        LazyColumn(verticalArrangement = Arrangement.spacedBy(18.dp)) {
            items(pages.books, key = { it.id }) { book ->
                NotebookSection(
                    book = book,
                    activeId = pages.active,
                    onOpenPage = actions.openPage,
                    onNewPage = { newPageIn = book.id },
                    onEditNotebook = { renamingBook = book },
                    onEditPage = { editingPage = it },
                )
            }
            item {
                TextButtonRow("＋ New notebook") { newNotebook = true }
            }
        }
    }

    HomeDialogs(
        pages = pages,
        actions = actions,
        intent =
            DialogIntent(
                newNotebook = newNotebook,
                newPageIn = newPageIn,
                renamingBook = renamingBook,
                editingPage = editingPage,
                clear = {
                    newNotebook = false
                    newPageIn = null
                    renamingBook = null
                    editingPage = null
                },
            ),
    )
}

/** Which dialog the home screen has open. */
private class DialogIntent(
    val newNotebook: Boolean,
    val newPageIn: String?,
    val renamingBook: PageBook?,
    val editingPage: PageMeta?,
    val clear: () -> Unit,
)

/** The four dialogs (new notebook, new page, notebook, page), rendered only while open. */
@Composable
private fun HomeDialogs(
    pages: PageList,
    actions: HomeActions,
    intent: DialogIntent,
) {
    if (intent.newNotebook) {
        PromptDialog(
            title = "New notebook",
            label = "Name",
            initial = "Notebook ${pages.books.size + 1}",
            confirm = "Create",
            onDismiss = intent.clear,
            onConfirm = {
                intent.clear()
                actions.newNotebook(it)
            },
        )
    }

    intent.newPageIn?.let { notebook ->
        NewPageDialog(
            suggested = pages.nextTitle(),
            onDismiss = intent.clear,
            onCreate = { title, template ->
                intent.clear()
                actions.newPage(title, template, notebook)
            },
        )
    }

    intent.renamingBook?.let { book ->
        PromptDialog(
            title = "Notebook",
            label = "Name",
            initial = book.title,
            confirm = "Save",
            extra =
                if (book.pages.isEmpty() && pages.books.size > 1) {
                    "Delete notebook" to { actions.deleteNotebook(book.id) }
                } else {
                    null
                },
            onDismiss = intent.clear,
            onConfirm = {
                intent.clear()
                actions.renameNotebook(book.id, it)
            },
        )
    }

    intent.editingPage?.let { page ->
        PromptDialog(
            title = page.title,
            label = "Page name",
            initial = page.title,
            confirm = "Save",
            extra = if (pages.all().size > 1) "Delete page" to { actions.deletePage(page.id) } else null,
            onDismiss = intent.clear,
            onConfirm = {
                intent.clear()
                actions.renamePage(page.id, it)
            },
        )
    }
}

@Composable
private fun Header(
    pageCount: Int,
    notebookCount: Int,
    onNewNotebook: () -> Unit,
    onSync: () -> Unit,
) {
    Row(verticalAlignment = Alignment.CenterVertically) {
        Column(Modifier.weight(1f)) {
            BasicText(
                "Penwise",
                style = TextStyle(fontSize = 26.sp, fontWeight = FontWeight.SemiBold, color = Color(INK)),
            )
            BasicText(
                "$notebookCount notebooks · $pageCount pages",
                style = TextStyle(fontSize = 13.sp, color = Color(MUTED)),
            )
        }
        GlyphChip(Glyph.Plus, label = "New notebook", selected = false, onClick = onNewNotebook)
        Spacer(Modifier.width(8.dp))
        GlyphChip(Glyph.Sync, label = "Sync settings", selected = false, onClick = onSync)
    }
}

@Composable
private fun NotebookSection(
    book: PageBook,
    activeId: String,
    onOpenPage: (String) -> Unit,
    onNewPage: () -> Unit,
    onEditNotebook: () -> Unit,
    onEditPage: (PageMeta) -> Unit,
) {
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            BasicText(
                book.title,
                style = TextStyle(fontSize = 15.sp, fontWeight = FontWeight.Medium, color = Color(INK)),
            )
            Spacer(Modifier.width(8.dp))
            BasicText(
                "${book.pages.size}",
                style = TextStyle(fontSize = 12.sp, color = Color(MUTED)),
            )
            Spacer(Modifier.weight(1f))
            GlyphChip(Glyph.Pencil, label = "Rename notebook", selected = false, onClick = onEditNotebook)
            Spacer(Modifier.width(4.dp))
            GlyphChip(Glyph.Plus, label = "New page", selected = false, onClick = onNewPage)
        }
        if (book.pages.isEmpty()) {
            EmptyRow("No pages yet — add one with ＋")
        } else {
            book.pages.chunked(COLUMNS).forEach { row ->
                Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                    row.forEach { page ->
                        PageCard(
                            page = page,
                            open = page.id == activeId,
                            onClick = { onOpenPage(page.id) },
                            onLongClick = { onEditPage(page) },
                            modifier = Modifier.weight(1f),
                        )
                    }
                    repeat(COLUMNS - row.size) { Spacer(Modifier.weight(1f)) }
                }
            }
        }
    }
}

private const val COLUMNS = 3

@Composable
private fun PageCard(
    page: PageMeta,
    open: Boolean,
    onClick: () -> Unit,
    onLongClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Column(
        modifier
            .background(Color(CARD), RoundedCornerShape(RADIUS_DP.dp))
            .combinedClickable(onClick = onClick, onLongClick = onLongClick)
            .padding(12.dp),
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            BasicText(page.glyph, style = TextStyle(fontSize = 15.sp, color = Color(MUTED)))
            Spacer(Modifier.weight(1f))
            Box(
                Modifier
                    .size(8.dp)
                    .background(
                        if (open) Color(ACCENT) else Color(0x00000000),
                        RoundedCornerShape(4.dp),
                    ),
            )
        }
        BasicText(
            page.title,
            style = TextStyle(fontSize = 14.sp, color = Color(INK), fontWeight = FontWeight.Medium),
            maxLines = 1,
        )
        BasicText(
            describe(page),
            style = TextStyle(fontSize = 11.sp, color = Color(MUTED)),
            maxLines = 1,
        )
    }
}

/** One line of context for a page card. */
private fun describe(page: PageMeta): String =
    when {
        page.infinite -> "Infinite canvas"
        page.background == "ruled" -> "Ruled A4"
        page.background == "grid" -> "Grid A4"
        page.background == "dots" -> "Dot grid A4"
        else -> "Blank A4"
    }

@Composable
private fun EmptyRow(text: String) {
    Box(
        Modifier
            .fillMaxWidth()
            .background(Color(SUBTLE_FILL), RoundedCornerShape(RADIUS_DP.dp))
            .padding(18.dp),
    ) {
        BasicText(text, style = TextStyle(fontSize = 13.sp, color = Color(MUTED)))
    }
}

@Composable
private fun TextButtonRow(
    label: String,
    onClick: () -> Unit,
) {
    Box(
        Modifier
            .background(Color(ACCENT_FILL), RoundedCornerShape(RADIUS_DP.dp))
            .clickable(onClick = onClick)
            .padding(horizontal = 16.dp, vertical = 10.dp),
    ) {
        BasicText(label, style = TextStyle(fontSize = 14.sp, color = Color(ACCENT)))
    }
}

/** Small dark dialog with one field and up to three actions. */
@Composable
private fun PromptDialog(
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
            BasicText(title, style = TextStyle(fontSize = 16.sp, color = Color(PANEL_TEXT)))
            BasicText(label, style = TextStyle(fontSize = 11.sp, color = Color(PANEL_MUTED)))
            BasicTextField(
                value = text,
                onValueChange = { text = it },
                singleLine = true,
                textStyle = TextStyle(fontSize = 15.sp, color = Color.White),
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

@Composable
private fun PanelButton(
    label: String,
    primary: Boolean,
    onClick: () -> Unit,
) {
    Box(
        Modifier
            .background(
                if (primary) Color(ACCENT) else Color(PANEL_BUTTON),
                RoundedCornerShape(8.dp),
            ).clickable(onClick = onClick)
            .padding(horizontal = 14.dp, vertical = 8.dp),
    ) {
        BasicText(
            label,
            style = TextStyle(fontSize = 14.sp, color = if (primary) Color.White else Color(PANEL_TEXT)),
        )
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
            BasicText("New page", style = TextStyle(fontSize = 16.sp, color = Color(PANEL_TEXT)))
            BasicTextField(
                value = title,
                onValueChange = { title = it },
                singleLine = true,
                textStyle = TextStyle(fontSize = 15.sp, color = Color.White),
                modifier =
                    Modifier
                        .fillMaxWidth()
                        .background(Color(FIELD), RoundedCornerShape(8.dp))
                        .padding(horizontal = 10.dp, vertical = 9.dp),
                decorationBox = { inner ->
                    if (title.isEmpty()) {
                        BasicText(suggested, style = TextStyle(fontSize = 15.sp, color = Color(PANEL_MUTED)))
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
                    BasicText(option.hint, style = TextStyle(fontSize = 11.sp, color = Color(PANEL_MUTED)))
                }
            }
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                PanelButton("Create", primary = true) { onCreate(title.ifBlank { suggested }, template) }
                PanelButton("Cancel", primary = false, onClick = onDismiss)
            }
        }
    }
}
