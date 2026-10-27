package dev.penwise.sync

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** Drives the real Rust `SyncCore` through the host `libpenwise_ffi.so`. */
class SyncCoreTest {
    private fun device(byte: Int) = ByteArray(16) { (byte + it).toByte() }.also { it[0] = 0 }

    @Test
    fun authoringShowsUpInTheCatalogAndTheOutbox() {
        SyncCore.create(device(1), 7L).use { core ->
            val notebook = core.createNotebook("Notes", 0)
            val page = core.createPage(notebook, "First", 0, floatArrayOf(), background = 1)
            val catalog = core.catalog()
            assertEquals(listOf("Notes"), catalog.liveNotebooks.map { it.title })
            val row = catalog.page(page)
            assertEquals("First", row?.title)
            assertEquals(notebook, row?.notebook)
            assertEquals(true, row?.infinite)
            assertEquals(1, row?.background)
            assertEquals(OutboxCounts(pending = 2, deadLetters = 0), core.outbox())
        }
    }

    @Test
    fun deletedPagesLeaveTheReadingOrder() {
        SyncCore.create(device(2), 9L).use { core ->
            val notebook = core.createNotebook("N", 0)
            val keep = core.createPage(notebook, "keep", 0, floatArrayOf(1000f, 800f), background = 0)
            val drop = core.createPage(notebook, "drop", 1, floatArrayOf(), background = 0)
            core.updatePage(drop, deleted = true)
            val catalog = core.catalog()
            assertEquals(listOf(keep), catalog.readingOrder().map { it.id })
            assertTrue(catalog.page(drop)?.deleted == true)
            assertEquals(1000f, catalog.page(keep)?.width)
        }
    }
}
