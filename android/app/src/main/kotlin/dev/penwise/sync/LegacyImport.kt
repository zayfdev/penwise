package dev.penwise.sync

import org.json.JSONArray
import org.json.JSONObject
import java.io.File

private const val PAGES_DIR = "pages"
private const val OLDEST_FILE = "page0.bin"
private const val DEFAULT_NOTEBOOK = "nb_local"
private const val DEFAULT_PAGE = "pg_local"

/** How far [LegacyImport.runWhenCaughtUp] got. */
enum class LegacyResult {
    /** The replica is not caught up with the server yet; nothing was imported, the import stays pending. */
    WAITING,

    /** Imported, but the state file could not be written; the marker was not written, the import stays pending. */
    NOT_DURABLE,

    /** Imported, durable, marker written. */
    DONE,
}

/** What a legacy import found and did. */
data class LegacyReport(
    val pages: Int,
    val added: Int,
    val alreadyPresent: Int,
    val skipped: Int,
    /** Strokes that shared a seed with an earlier one in their file and were kept under distinct identities. */
    val seedCollisions: Int = 0,
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

    /**
     * The only way the app imports: runs [run] when the core's own signal says the replica holds everything the
     * server announced ([SyncCore.caughtUp]), then waits for [flush] (the state file write; true on success) and
     * only then writes the marker. Anything else leaves the import pending, so a timeout, an unreachable server or
     * a failing disk can never end with the marker written and the ops lost.
     *
     * Importing before catch-up would stamp the legacy ops with local stamps the server's migrated copies never
     * match, so every edit other devices made before the import would lose against it on this tablet.
     */
    fun runWhenCaughtUp(
        core: SyncCore,
        flush: () -> Boolean,
    ): LegacyResult {
        if (!core.caughtUp()) return LegacyResult.WAITING
        run(core)
        if (!flush()) return LegacyResult.NOT_DURABLE
        stateDir.mkdirs()
        marker.writeText("done") // only now: the imported ops are confirmed on disk
        return LegacyResult.DONE
    }

    /** Imports everything; call only once caught up (see [runWhenCaughtUp]). Safe to repeat (everything dedupes). */
    fun run(core: SyncCore): LegacyReport {
        var pages = 0
        var added = 0
        var present = 0
        var skipped = 0
        var collisions = 0
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
                collisions += report.seedCollisions
            }
        }
        return LegacyReport(pages, added, present, skipped, collisions)
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
