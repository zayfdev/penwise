package dev.penwise.ink

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.launch

/** Notebook create/rename/delete: applied to the index at once, persisted and sent to the server after. */
internal class NotebookActions(
    private val library: PageLibrary,
    private val pages: MutableStateFlow<PageList>,
    private val scope: CoroutineScope,
) {
    fun create(title: String) {
        val trimmed = title.trim().ifBlank { "Notebook ${pages.value.books.size + 1}" }
        val book = PageBook(library.newNotebookId(), trimmed, emptyList())
        val list = pages.value.withNotebook(book.id, book.title)
        pages.value = list
        scope.launch(Dispatchers.IO) {
            library.save(list)
            library.createNotebookRemote(book.title)
        }
    }

    fun rename(
        id: String,
        title: String,
    ) {
        val trimmed = title.trim()
        if (trimmed.isEmpty()) return
        val list = pages.value.withRenamedNotebook(id, trimmed)
        pages.value = list
        scope.launch(Dispatchers.IO) {
            library.save(list)
            library.renameNotebookRemote(id, trimmed)
        }
    }

    fun delete(id: String) {
        val before = pages.value
        val after = before.withoutNotebook(id)
        if (after.books.size == before.books.size) return
        pages.value = after
        scope.launch(Dispatchers.IO) {
            library.save(after)
            library.deleteNotebookRemote(id)
        }
    }
}
