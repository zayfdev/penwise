package dev.penwise.ink

import org.json.JSONArray
import org.json.JSONObject
import java.io.File

/** One page's settings, matching the server's `meta.json` shape. */
data class PageMeta(
    val id: String,
    val title: String,
    val infinite: Boolean,
    val background: String,
) {
    /** Single-character glyph for the page chip. */
    val glyph: String
        get() =
            when {
                infinite -> "∞"
                background == "ruled" -> "☰"
                background == "grid" -> "▦"
                background == "dots" -> "⋯"
                else -> "▢"
            }
}

/** A named group of pages. */
data class PageBook(
    val id: String,
    val title: String,
    val pages: List<PageMeta>,
)

/** The whole library index: what the server sends as `meta` and what the app caches on disk. */
data class PageList(
    val active: String,
    val books: List<PageBook>,
) {
    /** Every page in reading order. */
    fun all(): List<PageMeta> = books.flatMap { it.pages }

    /** The page with `id`, if it is still in the library. */
    fun page(id: String): PageMeta? = all().firstOrNull { it.id == id }

    /** The page shown by default: [active] when it exists, else the first page. */
    fun startPage(): PageMeta? = page(active) ?: all().firstOrNull()

    /** Title for the next appended page. */
    fun nextTitle(): String = "Page ${all().size + 1}"

    /** A copy with `page` appended to the notebook holding [active] (or the first notebook). */
    fun withPage(
        page: PageMeta,
        notebook: String? = null,
    ): PageList {
        val holder =
            notebook?.let { id -> books.firstOrNull { it.id == id } }
                ?: books.firstOrNull { book -> book.pages.any { it.id == active } }
                ?: books.firstOrNull()
                ?: return copy(books = listOf(PageBook(NEW_NOTEBOOK_ID, "Notebook 1", listOf(page))))
        val next =
            books.map { book ->
                if (book.id == holder.id) book.copy(pages = book.pages + page) else book
            }
        return copy(books = next)
    }

    /** A copy without `page`; returns the receiver when it is the last page. */
    fun withoutPage(id: String): PageList {
        if (all().size <= 1) return this
        val next = books.map { book -> book.copy(pages = book.pages.filter { it.id != id }) }
        val survivor = next.flatMap { it.pages }.firstOrNull()?.id ?: active
        return PageList(active = if (active == id) survivor else active, books = next)
    }

    /** A copy with a new (empty) notebook appended. */
    fun withNotebook(
        id: String,
        title: String,
    ): PageList = copy(books = books + PageBook(id, title, emptyList()))

    /** A copy with `id`'s title changed. */
    fun withRenamedNotebook(
        id: String,
        title: String,
    ): PageList = copy(books = books.map { if (it.id == id) it.copy(title = title) else it })

    /** A copy without an empty notebook; returns the receiver when it has pages or is the last one. */
    fun withoutNotebook(id: String): PageList {
        val target = books.firstOrNull { it.id == id } ?: return this
        if (target.pages.isNotEmpty() || books.size <= 1) return this
        return copy(books = books.filterNot { it.id == id })
    }

    /** A copy with `page` replaced by [changed] (title or background edits). */
    fun withUpdated(changed: PageMeta): PageList =
        copy(
            books =
                books.map { book ->
                    book.copy(pages = book.pages.map { if (it.id == changed.id) changed else it })
                },
        )

    companion object {
        const val NEW_NOTEBOOK_ID = "nb_local"
        const val DEFAULT_PAGE_ID = "pg_local"

        /** First-run library: one A4 page titled `Page 1`. */
        fun firstRun(): PageList =
            PageList(
                active = DEFAULT_PAGE_ID,
                books =
                    listOf(
                        PageBook(
                            id = NEW_NOTEBOOK_ID,
                            title = "Notebook 1",
                            pages = listOf(PageMeta(DEFAULT_PAGE_ID, "Page 1", infinite = false, background = "blank")),
                        ),
                    ),
            )
    }
}

/** JSON codec for [PageList]; the same shape the server writes to `meta.json` and sends in `meta` frames. */
object PageListCodec {
    private const val VERSION = 1

    /** Parses a library object; `null` when the text is not one. */
    fun read(text: String): PageList? =
        runCatching {
            val root = JSONObject(text)
            read(root)
        }.getOrNull()

    /** Parses an already-decoded object (the payload of a `meta` frame). */
    fun read(root: JSONObject): PageList {
        val books = root.optJSONArray("notebooks") ?: JSONArray()
        val parsed =
            (0 until books.length()).mapNotNull { index ->
                runCatching { book(books.getJSONObject(index)) }.getOrNull()
            }
        val fallback = PageList.firstRun()
        if (parsed.isEmpty()) return fallback
        val active = root.optString("active").takeIf { id -> parsed.any { book -> book.pages.any { it.id == id } } }
        return PageList(
            active =
                active ?: parsed
                    .first()
                    .pages
                    .firstOrNull()
                    ?.id
                    .orEmpty(),
            books = parsed,
        )
    }

    /** Serializes the library in the shape the server accepts. */
    fun write(list: PageList): String {
        val root = JSONObject().put("version", VERSION).put("active", list.active)
        val books = JSONArray()
        list.books.forEach { book ->
            val pages = JSONArray()
            book.pages.forEach { pages.put(page(it)) }
            books.put(JSONObject().put("id", book.id).put("title", book.title).put("pages", pages))
        }
        return root.put("notebooks", books).toString()
    }

    private fun book(json: JSONObject): PageBook {
        val pages = json.optJSONArray("pages") ?: JSONArray()
        return PageBook(
            id = json.getString("id"),
            title = json.optString("title", "Notebook"),
            pages =
                (0 until pages.length()).mapNotNull { index ->
                    runCatching { page(pages.getJSONObject(index)) }.getOrNull()
                },
        )
    }

    private fun page(json: JSONObject): PageMeta {
        val kind = json.optJSONObject("kind")
        return PageMeta(
            id = json.getString("id"),
            title = json.optString("title", "Page"),
            infinite = kind?.optString("t") == "infinite",
            background = json.optString("background", "blank"),
        )
    }

    private fun page(page: PageMeta): JSONObject {
        val kind =
            if (page.infinite) {
                JSONObject().put("t", "infinite")
            } else {
                JSONObject().put("t", "fixed").put("width", A4_WIDTH).put("height", A4_HEIGHT)
            }
        return JSONObject()
            .put("id", page.id)
            .put("title", page.title)
            .put("kind", kind)
            .put("background", page.background)
    }

    /** A4 portrait in page px, matching the server's default. */
    const val A4_WIDTH = 1600.0
    const val A4_HEIGHT = 2560.0
}

/** The library cache in app files: `meta.json` plus one stroke file per page. */
class PageStore(
    private val dir: File,
) {
    private companion object {
        /** Hex characters in an id; matches the server's `pg_` + 16 hex shape. */
        const val ID_DIGITS = 16
        const val LEGACY_FILE_NAME = "page0.bin"
    }

    private val metaFile: File get() = File(dir, "meta.json")

    /** The cached library, or a fresh single-page one when nothing is stored yet. */
    fun load(): PageList {
        val text = runCatching { metaFile.readText() }.getOrNull()
        val list = text?.let { PageListCodec.read(it) } ?: PageList.firstRun()
        migrateLegacy(list)
        return list
    }

    /**
     * Pre-pages builds kept every stroke in `page0.bin`; its content belongs to the first page of a library that
     * has no per-page files yet. The old file is copied, never removed.
     */
    private fun migrateLegacy(list: PageList) {
        val legacy = File(dir, LEGACY_FILE_NAME)
        val first = list.all().firstOrNull() ?: return
        val target = File(File(dir, StrokeFile.PAGES_DIR), "${first.id}.bin")
        if (!legacy.isFile || target.isFile) return
        target.parentFile?.mkdirs()
        runCatching { legacy.copyTo(target) }
    }

    /** Writes the library atomically; failures leave the previous file intact. */
    fun save(list: PageList) {
        dir.mkdirs()
        val staging = File(dir, "meta.json.tmp")
        staging.writeText(PageListCodec.write(list))
        if (!staging.renameTo(metaFile)) {
            staging.copyTo(metaFile, overwrite = true)
            staging.delete()
        }
    }

    /** Stroke storage for one page. */
    fun strokes(pageId: String): StrokeFile = StrokeFile(dir, pageId)

    /** A fresh page id, unique enough without a server round trip. */
    fun newPageId(): String =
        "pg_" +
            java.util.UUID
                .randomUUID()
                .toString()
                .replace("-", "")
                .take(ID_DIGITS)
}
