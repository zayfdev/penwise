package dev.penwise.ink

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** Notebook grouping: creation, renaming, and the rule that only empty notebooks can go. */
class PageListNotebooksTest {
    private fun page(id: String) = PageMeta(id, "Page", infinite = false, background = "blank")

    private fun library() =
        PageList(
            active = "pg_1",
            books = listOf(PageBook("nb_1", "Notes", listOf(page("pg_1")))),
        )

    @Test
    fun a_new_notebook_is_appended_empty() {
        val grown = library().withNotebook("nb_2", "Recipes")
        assertEquals(listOf("Notes", "Recipes"), grown.books.map { it.title })
        assertTrue(grown.books[1].pages.isEmpty())
        assertEquals("the open page is untouched", "pg_1", grown.active)
    }

    @Test
    fun renaming_changes_only_that_notebook() {
        val two = library().withNotebook("nb_2", "Recipes")
        val renamed = two.withRenamedNotebook("nb_2", "Cooking")
        assertEquals(listOf("Notes", "Cooking"), renamed.books.map { it.title })
    }

    @Test
    fun only_empty_notebooks_can_be_deleted() {
        val two = library().withNotebook("nb_2", "Recipes")
        val afterEmpty = two.withoutNotebook("nb_2")
        assertEquals(1, afterEmpty.books.size)
        assertTrue("a notebook with pages stays", two.withoutNotebook("nb_1") === two)
        val onlyOne = PageList(active = "", books = listOf(PageBook("nb_only", "Only", emptyList())))
        assertTrue("the last notebook stays", onlyOne.withoutNotebook("nb_only") === onlyOne)
    }

    @Test
    fun an_unknown_notebook_is_a_no_op() {
        val list = library()
        assertTrue(list.withoutNotebook("nb_missing") === list)
        assertEquals(list.books, list.withRenamedNotebook("nb_missing", "x").books.filterNot { it.id == "nb_missing" })
    }
}
