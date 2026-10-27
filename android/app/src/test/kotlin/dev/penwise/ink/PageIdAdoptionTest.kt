package dev.penwise.ink

import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/** The first-run `pg_local` page takes the id the server mints for it instead of being pushed forever. */
class PageIdAdoptionTest {
    private val root = File(System.getProperty("java.io.tmpdir"), "penwise-adopt-${System.nanoTime()}")

    @After
    fun cleanUp() {
        root.deleteRecursively()
    }

    @Test
    fun the_server_id_is_read_from_the_create_response() {
        assertEquals("pg_0123456789abcdef", PageIdAdoption.idOf("""{"id":"pg_0123456789abcdef","title":"Page 1"}"""))
        assertNull(PageIdAdoption.idOf(null))
        assertNull(PageIdAdoption.idOf("not json"))
        assertNull(PageIdAdoption.idOf("""{"title":"x"}"""))
    }

    @Test
    fun renaming_moves_the_page_and_the_active_pointer() {
        val list = PageList.firstRun()
        val renamed = PageIdAdoption.renamed(list, PageList.DEFAULT_PAGE_ID, "pg_0123456789abcdef")
        assertEquals("pg_0123456789abcdef", renamed.active)
        assertEquals(listOf("pg_0123456789abcdef"), renamed.all().map { it.id })
        assertEquals("Page 1", renamed.page("pg_0123456789abcdef")?.title)
    }

    @Test
    fun the_stroke_and_undo_files_follow_the_new_id() {
        val pages = File(root, StrokeFile.PAGES_DIR).apply { mkdirs() }
        File(pages, "pg_local.bin").writeText("strokes")
        File(pages, "pg_local.undo").writeText("undo")
        PageIdAdoption.moveFiles(root, "pg_local", "pg_new")
        assertEquals("strokes", File(pages, "pg_new.bin").readText())
        assertEquals("undo", File(pages, "pg_new.undo").readText())
        assertFalse(File(pages, "pg_local.bin").exists())
        assertTrue(File(pages, "pg_new.undo").isFile)
    }
}
