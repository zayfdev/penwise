package dev.penwise.sync

import java.nio.ByteBuffer
import java.nio.ByteOrder

private const val ID_BYTES = 16

/** One notebook row of the replica's catalog. */
data class CatalogNotebook(
    val id: String,
    val title: String,
    val position: Long,
    val deleted: Boolean,
)

/** One page row of the replica's catalog. [background] is `Background::code`: 0 blank, 1 lined, 2 grid, 3 dot. */
data class CatalogPage(
    val id: String,
    val notebook: String,
    val title: String,
    val position: Long,
    val deleted: Boolean,
    val infinite: Boolean,
    val width: Float,
    val height: Float,
    val background: Int,
)

/**
 * The notebook and page lists of the replica (`sync_core/catalog_blob.rs`), tombstones included. Notebooks and pages
 * keep the catalog's `(position, id)` order.
 */
class Catalog(
    val notebooks: List<CatalogNotebook>,
    val pages: List<CatalogPage>,
) {
    /** Live notebooks. */
    val liveNotebooks: List<CatalogNotebook> get() = notebooks.filterNot { it.deleted }

    /** Live pages of live notebooks, in reading order. */
    fun livePages(notebook: String): List<CatalogPage> = pages.filter { it.notebook == notebook && !it.deleted }

    /** Every live page in reading order (notebook order, then page order). */
    fun readingOrder(): List<CatalogPage> = liveNotebooks.flatMap { livePages(it.id) }

    fun page(id: String): CatalogPage? = pages.firstOrNull { it.id == id }

    companion object {
        /** Parses a `syncCatalog` blob. */
        fun parse(blob: ByteArray): Catalog {
            val buffer = ByteBuffer.wrap(blob).order(ByteOrder.LITTLE_ENDIAN)
            val notebooks =
                List(buffer.getInt()) {
                    CatalogNotebook(
                        id = id(buffer),
                        deleted = buffer.get().toInt() != 0,
                        position = buffer.getLong(),
                        title = text(buffer),
                    )
                }
            val pages = List(buffer.getInt()) { page(buffer) }
            return Catalog(notebooks, pages)
        }

        private fun page(buffer: ByteBuffer): CatalogPage {
            val id = id(buffer)
            val notebook = id(buffer)
            val deleted = buffer.get().toInt() != 0
            val position = buffer.getLong()
            val infinite = buffer.get().toInt() == 1
            val width = buffer.getFloat()
            val height = buffer.getFloat()
            val background = buffer.get().toInt() and 0xFF
            return CatalogPage(id, notebook, text(buffer), position, deleted, infinite, width, height, background)
        }

        private fun id(buffer: ByteBuffer): String = Ids.hex(ByteArray(ID_BYTES).also(buffer::get))

        private fun text(buffer: ByteBuffer): String =
            String(ByteArray(buffer.getInt()).also(buffer::get), Charsets.UTF_8)
    }
}
