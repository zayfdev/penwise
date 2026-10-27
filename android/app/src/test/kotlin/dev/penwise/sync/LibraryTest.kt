package dev.penwise.sync

import dev.penwise.ink.PageTemplate
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** Notebook and page rules, through the real core's catalog. */
class LibraryTest {
    private val tablet = Device(server = null)

    @After
    fun close() = tablet.close()

    private val library get() = tablet.library

    @Test
    fun first_run_creates_one_blank_a4_page() {
        val id = library.createDefault()
        val list = library.list(null)
        assertEquals(id, list.active)
        assertEquals(listOf("Notebook 1"), list.books.map { it.title })
        val page = requireNotNull(list.page(id))
        assertEquals("Page 1", page.title)
        assertFalse(page.infinite)
        assertEquals(SHEET_WIDTH, page.width, 0f)
    }

    @Test
    fun new_pages_append_in_the_chosen_notebook_with_their_template() {
        library.createDefault()
        val second = library.newNotebook("  Ideas ")
        val grid = requireNotNull(library.newPage("", PageTemplate.Grid, second))
        val infinite = requireNotNull(library.newPage("Wide", PageTemplate.Infinite, second))
        val list = library.list(null)
        assertEquals(listOf("Notebook 1", "Ideas"), list.books.map { it.title })
        assertEquals(listOf(grid, infinite), list.books[1].pages.map { it.id })
        assertEquals("a blank title falls back to a numbered one", "Page 2", list.page(grid)?.title)
        assertEquals("grid", list.page(grid)?.background)
        assertTrue(list.page(infinite)?.infinite == true)
    }

    @Test
    fun the_last_page_and_non_empty_or_last_notebooks_cannot_be_deleted() {
        val only = library.createDefault()
        assertFalse(library.deletePage(only))
        val first =
            library
                .list(null)
                .books
                .single()
                .id
        assertFalse("the last notebook stays", library.deleteNotebook(first))

        val second = library.newNotebook("Spare")
        assertFalse("a notebook with pages stays", library.deleteNotebook(first))
        assertTrue(library.deleteNotebook(second))
        val extra = requireNotNull(library.newPage("Extra", PageTemplate.Blank, first))
        assertTrue(library.deletePage(extra))
        assertEquals(listOf(only), library.list(null).all().map { it.id })
    }

    @Test
    fun renames_apply_and_a_stale_active_page_falls_back_to_the_first() {
        val first = library.createDefault()
        val second = requireNotNull(library.newPage("Two", PageTemplate.Blank, null))
        library.renamePage(second, "Chapter")
        library.renamePage(second, "   ")
        assertEquals("Chapter", library.list(second).page(second)?.title)
        assertEquals(second, library.list(second).active)
        library.deletePage(second)
        assertEquals(first, library.list(second).active)
        assertNull(library.list(second).page(second))
    }
}
