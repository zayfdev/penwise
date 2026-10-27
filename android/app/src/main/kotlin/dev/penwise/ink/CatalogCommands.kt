package dev.penwise.ink

import dev.penwise.sync.Library
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/**
 * The notebook and page changes the UI asks for. Each takes the core lock, so each runs on [dispatcher] (one thread,
 * so a create followed by a rename keeps its order) and republishes the catalog afterwards; the UI thread only
 * posts the request.
 */
class CatalogCommands(
    private val scope: CoroutineScope,
    private val dispatcher: CoroutineDispatcher,
    private val library: () -> Library?,
    private val pages: () -> PageList,
    private val publish: (active: String?) -> Unit,
) {
    private fun change(change: (Library) -> Unit) {
        scope.launch(dispatcher) {
            val open = library() ?: return@launch
            change(open)
            publish(null)
        }
    }

    /**
     * Creates a page and switches to it; [created] gets its id (on the main thread) so the caller can navigate at
     * once. `notebook` defaults to the first one.
     */
    fun newPage(
        title: String,
        template: PageTemplate,
        notebook: String? = null,
        created: (String) -> Unit = {},
    ) {
        val list = pages()
        val target = notebook ?: list.books.firstOrNull { book -> book.pages.any { it.id == list.active } }?.id
        scope.launch(dispatcher) {
            val id = library()?.newPage(title, template, target) ?: return@launch
            publish(id)
            withContext(Dispatchers.Main) { created(id) }
        }
    }

    fun renamePage(
        id: String,
        title: String,
    ) = change { it.renamePage(id, title) }

    /** Deletes a page (never the last one). */
    fun deletePage(id: String) = change { it.deletePage(id) }

    fun createNotebook(title: String) = change { it.newNotebook(title) }

    fun renameNotebook(
        id: String,
        title: String,
    ) = change { it.renameNotebook(id, title) }

    /** Deletes an empty notebook (the last one and non-empty ones are refused). */
    fun deleteNotebook(id: String) = change { it.deleteNotebook(id) }
}
