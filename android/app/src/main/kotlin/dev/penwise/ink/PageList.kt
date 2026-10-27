package dev.penwise.ink

/** One page as the UI shows it (a row of the replica's catalog). */
data class PageMeta(
    val id: String,
    val title: String,
    val infinite: Boolean,
    val background: String,
    /** Sheet size in page px; zero for infinite pages. */
    val width: Float = 0f,
    val height: Float = 0f,
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

/** The library as the UI sees it: live notebooks with their live pages, and the page on screen. */
data class PageList(
    val active: String,
    val books: List<PageBook>,
) {
    /** Every page in reading order. */
    fun all(): List<PageMeta> = books.flatMap { it.pages }

    /** The page with `id`, if it is in the library. */
    fun page(id: String): PageMeta? = all().firstOrNull { it.id == id }

    /** The page shown by default: [active] when it exists, else the first page. */
    fun startPage(): PageMeta? = page(active) ?: all().firstOrNull()

    /** Title for the next appended page. */
    fun nextTitle(): String = "Page ${all().size + 1}"

    companion object {
        /** Nothing loaded yet. */
        val EMPTY = PageList(active = "", books = emptyList())
    }
}
