package dev.penwise.sync

import org.json.JSONArray
import org.json.JSONObject
import java.io.File

private const val PAGES_DIR = "pages"
private const val OLDEST_FILE = "page0.bin"
private const val DEFAULT_NOTEBOOK = "nb_local"
private const val DEFAULT_PAGE = "pg_local"

/** What a legacy import found and did. */
data class LegacyReport(
    val pages: Int,
    val added: Int,
    val alreadyPresent: Int,
    val skipped: Int,
)

private class LegacyPage(
    val id: String,
    val title: String,
    val infinite: Boolean,
    val background: String,
)

private class LegacyBook(
    val id: String,
    val title: String,
    val pages: List<LegacyPage>,
)

/**
 * One-time import of the pre-rewrite app files: the library index (`meta.json`) and the per-page stroke files
 * (`pages/<id>.bin`, `page0.bin` from the oldest builds). The stroke parsing and conversion are Rust
 * ([SyncCore.importLegacyPage]); this class only finds the files and makes sure each page exists under the
 * deterministic id every device derives, so the server's migrated copy and this import union by op id.
 *
 * The old files are never deleted: they stay as a backup. [done] is a marker file written after the imported ops
 * are durable.
 */
class LegacyImport(
    private val filesDir: File,
    private val stateDir: File,
) {
    private val marker = File(stateDir, "legacy-import.done")

    /** True when old files exist and the import has not completed yet. */
    fun pending(): Boolean = !marker.exists() && (File(filesDir, "meta.json").isFile || pageFileIds().isNotEmpty())

    /** Imports everything; call after the first catch-up. Safe to repeat (everything dedupes). */
    fun run(core: SyncCore): LegacyReport {
        var pages = 0
        var added = 0
        var present = 0
        var skipped = 0
        var position = 0L
        for (book in books()) {
            val notebook = ensureNotebook(core, book, position++)
            for ((index, page) in book.pages.withIndex()) {
                ensurePage(core, notebook, page, index.toLong())
                pages++
                val bytes = strokeBytes(page.id, first = pages == 1) ?: continue
                val report = core.importLegacyPage(page.id, bytes)
                added += report.added
                present += report.alreadyPresent
                skipped += report.skipped
            }
        }
        return LegacyReport(pages, added, present, skipped)
    }

    /** Marks the import complete (call after the state file holds the imported ops). */
    fun done() {
        stateDir.mkdirs()
        marker.writeText("done")
    }

    private fun ensureNotebook(
        core: SyncCore,
        book: LegacyBook,
        position: Long,
    ): String {
        val id = SyncCore.legacyNotebookId(book.id)
        if (core.catalog().notebooks.none { it.id == id }) core.createNotebook(book.title, position, id)
        return id
    }

    private fun ensurePage(
        core: SyncCore,
        notebook: String,
        page: LegacyPage,
        position: Long,
    ) {
        val id = SyncCore.legacyPageId(page.id)
        if (core.catalog().page(id) != null) return
        val size = if (page.infinite) floatArrayOf() else floatArrayOf(SHEET_WIDTH, SHEET_HEIGHT)
        core.createPage(notebook, page.title, position, size, Backgrounds.code(page.background), id)
    }

    /** The old index when readable; otherwise one notebook holding every page file found. */
    private fun books(): List<LegacyBook> {
        val parsed = runCatching { parseIndex(File(filesDir, "meta.json").readText()) }.getOrNull()
        if (!parsed.isNullOrEmpty()) return parsed
        val ids = pageFileIds().ifEmpty { listOf(DEFAULT_PAGE) }
        return listOf(LegacyBook(DEFAULT_NOTEBOOK, "Notebook 1", ids.map { LegacyPage(it, it, false, "blank") }))
    }

    /** Ids of the page files present. */
    private fun pageFileIds(): List<String> =
        File(filesDir, PAGES_DIR)
            .listFiles { file -> file.extension == "bin" }
            ?.map { it.nameWithoutExtension }
            ?.sorted()
            .orEmpty()

    /** The page's file; the oldest single-file layout stands in for the very first page. */
    private fun strokeBytes(
        id: String,
        first: Boolean,
    ): ByteArray? {
        val file = File(File(filesDir, PAGES_DIR), "$id.bin")
        if (file.isFile) return file.readBytes()
        val oldest = File(filesDir, OLDEST_FILE)
        return oldest.takeIf { first && it.isFile }?.readBytes()
    }

    private fun parseIndex(text: String): List<LegacyBook> {
        val books = JSONObject(text).optJSONArray("notebooks") ?: JSONArray()
        return (0 until books.length()).mapNotNull { index ->
            runCatching { book(books.getJSONObject(index)) }.getOrNull()
        }
    }

    private fun book(json: JSONObject): LegacyBook {
        val pages = json.optJSONArray("pages") ?: JSONArray()
        return LegacyBook(
            id = json.getString("id"),
            title = json.optString("title", "Notebook"),
            pages =
                (0 until pages.length()).mapNotNull { index ->
                    runCatching { page(pages.getJSONObject(index)) }.getOrNull()
                },
        )
    }

    private fun page(json: JSONObject): LegacyPage =
        LegacyPage(
            id = json.getString("id"),
            title = json.optString("title", "Page"),
            infinite = json.optJSONObject("kind")?.optString("t") == "infinite",
            background = json.optString("background", "blank"),
        )
}
