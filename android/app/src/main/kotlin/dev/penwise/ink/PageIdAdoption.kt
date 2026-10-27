package dev.penwise.ink

import org.json.JSONObject
import java.io.File

/**
 * Taking the id the server gives a page. The first-run page is `pg_local`, which the server refuses as a client
 * id and replaces with one it mints; the tablet then has to rename the page instead of pushing it forever.
 */
internal object PageIdAdoption {
    /** The `id` in a `POST /pages` response, or `null` when the call failed or the body is not a page. */
    fun idOf(response: String?): String? =
        response?.let { runCatching { JSONObject(it).getString("id") }.getOrNull() }?.takeIf { it.isNotEmpty() }

    /** `list` with page `from` renamed to `to`, active pointer included. */
    fun renamed(
        list: PageList,
        from: String,
        to: String,
    ): PageList =
        list.copy(
            active = if (list.active == from) to else list.active,
            books =
                list.books.map { book ->
                    book.copy(pages = book.pages.map { if (it.id == from) it.copy(id = to) else it })
                },
        )

    /** Moves the stroke and undo files of `from` to `to`; a missing file is simply nothing to move. */
    fun moveFiles(
        dir: File,
        from: String,
        to: String,
    ) {
        val pages = File(dir, StrokeFile.PAGES_DIR)
        for (extension in listOf("bin", "undo")) {
            val source = File(pages, "$from.$extension")
            if (source.isFile) source.renameTo(File(pages, "$to.$extension"))
        }
    }
}
