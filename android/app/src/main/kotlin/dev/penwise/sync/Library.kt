package dev.penwise.sync

import dev.penwise.ink.PageBook
import dev.penwise.ink.PageList
import dev.penwise.ink.PageMeta
import dev.penwise.ink.PageTemplate

/** Default sheet in page px: landscape, matching the tablet's screen (and the server's default). */
const val SHEET_WIDTH = 2560f
const val SHEET_HEIGHT = 1600f

private const val BACKGROUND_BLANK = 0
private const val BACKGROUND_LINED = 1
private const val BACKGROUND_GRID = 2
private const val BACKGROUND_DOT = 3

/** Names of the paper patterns the UI uses, and their `Background::code` values. */
object Backgrounds {
    fun code(name: String): Int =
        when (name) {
            "ruled" -> BACKGROUND_LINED
            "grid" -> BACKGROUND_GRID
            "dots" -> BACKGROUND_DOT
            else -> BACKGROUND_BLANK
        }

    fun name(code: Int): String =
        when (code) {
            BACKGROUND_LINED -> "ruled"
            BACKGROUND_GRID -> "grid"
            BACKGROUND_DOT -> "dots"
            else -> "blank"
        }
}

/**
 * Notebook and page create/rename/delete, all as ops on the [SyncCore] (so they sync like strokes), and the UI
 * list derived from the replica's catalog. The catalog is the only source of truth: nothing is cached here.
 */
class Library(
    private val core: SyncCore,
) {
    /** The UI model of the replica's catalog; [active] falls back to the first page when it is gone. */
    fun list(active: String?): PageList {
        val catalog = core.catalog()
        val books =
            catalog.liveNotebooks.map { book ->
                PageBook(book.id, book.title, catalog.livePages(book.id).map(::meta))
            }
        val first = books.firstNotNullOfOrNull { it.pages.firstOrNull() }?.id.orEmpty()
        val chosen = active?.takeIf { id -> books.any { book -> book.pages.any { it.id == id } } } ?: first
        return PageList(active = chosen, books = books)
    }

    private fun meta(page: CatalogPage) =
        PageMeta(
            id = page.id,
            title = page.title,
            infinite = page.infinite,
            background = Backgrounds.name(page.background),
            width = page.width,
            height = page.height,
        )

    /** First-run content: one notebook with one blank page. Returns the page id. */
    fun createDefault(): String {
        val notebook = core.createNotebook("Notebook 1", 0)
        return core.createPage(notebook, "Page 1", 0, floatArrayOf(SHEET_WIDTH, SHEET_HEIGHT), BACKGROUND_BLANK)
    }

    /** Appends a page to `notebook` (default: the first one); returns its id, or `null` when there is no notebook. */
    fun newPage(
        title: String,
        template: PageTemplate,
        notebook: String?,
    ): String? {
        val catalog = core.catalog()
        val target = notebook ?: catalog.liveNotebooks.firstOrNull()?.id ?: return null
        val position =
            catalog.pages
                .filter { it.notebook == target }
                .maxOfOrNull { it.position }
                ?.plus(1) ?: 0L
        val size = if (template.infinite) floatArrayOf() else floatArrayOf(SHEET_WIDTH, SHEET_HEIGHT)
        val name = title.trim().ifBlank { "Page ${catalog.readingOrder().size + 1}" }
        return core.createPage(target, name, position, size, Backgrounds.code(template.background))
    }

    fun renamePage(
        id: String,
        title: String,
    ) {
        val name = title.trim()
        if (name.isNotEmpty()) core.updatePage(id, title = name)
    }

    /** Deletes a page; the last remaining page is refused. Returns false when nothing was deleted. */
    fun deletePage(id: String): Boolean {
        val catalog = core.catalog()
        if (catalog.readingOrder().size <= 1 || catalog.readingOrder().none { it.id == id }) return false
        core.updatePage(id, deleted = true)
        return true
    }

    /** Appends a notebook; returns its id. */
    fun newNotebook(title: String): String {
        val catalog = core.catalog()
        val position = catalog.notebooks.maxOfOrNull { it.position }?.plus(1) ?: 0L
        val name = title.trim().ifBlank { "Notebook ${catalog.liveNotebooks.size + 1}" }
        return core.createNotebook(name, position)
    }

    fun renameNotebook(
        id: String,
        title: String,
    ) {
        val name = title.trim()
        if (name.isNotEmpty()) core.updateNotebook(id, title = name)
    }

    /** Deletes an empty notebook; non-empty ones and the last one are refused. Returns false when refused. */
    fun deleteNotebook(id: String): Boolean {
        val catalog = core.catalog()
        if (catalog.liveNotebooks.size <= 1 || catalog.livePages(id).isNotEmpty()) return false
        core.updateNotebook(id, deleted = true)
        return true
    }
}
