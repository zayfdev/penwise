package dev.penwise.ink

import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/** The page library: JSON shape, ordering, defaults, and the one-file-per-page fallback for older builds. */
class PageListTest {
    private fun page(
        id: String,
        title: String = "Page",
        background: String = "blank",
        infinite: Boolean = false,
    ) = PageMeta(id, title, infinite, background)

    private fun library(
        vararg pages: PageMeta,
        active: String? = null,
    ) = PageList(
        active = active ?: pages.first().id,
        books = listOf(PageBook("nb_1", "Notebook 1", pages.toList())),
    )

    @Test
    fun a_library_survives_a_json_round_trip() {
        val original =
            library(
                page("pg_a", "Notes", "ruled"),
                page("pg_b", "Sketch", infinite = true),
                active = "pg_b",
            )
        val parsed = PageListCodec.read(PageListCodec.write(original))
        assertNotNull(parsed)
        assertEquals("pg_b", parsed!!.active)
        assertEquals(listOf("pg_a", "pg_b"), parsed.all().map { it.id })
        assertEquals("ruled", parsed.page("pg_a")!!.background)
        assertTrue(parsed.page("pg_b")!!.infinite)
        assertEquals(listOf("☰", "∞"), parsed.all().map { it.glyph })
    }

    @Test
    fun server_meta_is_understood_including_page_kinds() {
        val meta =
            """
            {"version":1,"active":"pg_2","notebooks":[{"id":"nb_1","title":"Notebook 1","pages":[
              {"id":"pg_1","title":"Page 1","kind":{"t":"fixed","width":1600.0,"height":2560.0},"background":"grid"},
              {"id":"pg_2","title":"Page 2","kind":{"t":"infinite"},"background":"blank"}]}]}
            """.trimIndent()
        val parsed = PageListCodec.read(meta)!!
        assertEquals(2, parsed.all().size)
        assertEquals("grid", parsed.page("pg_1")!!.background)
        assertTrue(parsed.page("pg_2")!!.infinite)
        assertEquals("pg_2", parsed.active)
    }

    @Test
    fun malformed_or_empty_input_falls_back_to_one_page() {
        assertNull(PageListCodec.read("not json"))
        assertEquals(1, PageListCodec.read("""{"notebooks":[]}""")!!.all().size)
        // A vanished active page must not leave the strip pointing at nothing.
        val stale =
            PageListCodec.read(
                """{"active":"pg_gone","notebooks":[{"id":"nb","title":"N","pages":[{"id":"pg_1","title":"P"}]}]}""",
            )!!
        assertEquals("pg_1", stale.startPage()?.id)
        assertNull(stale.page("pg_gone"))
        assertEquals(
            "Page 1",
            PageList
                .firstRun()
                .all()
                .single()
                .title,
        )
    }

    @Test
    fun new_pages_are_titled_by_position_and_appended_to_the_active_notebook() {
        val start = PageList.firstRun()
        assertEquals("Page 2", start.nextTitle())
        val grown = start.withPage(page("pg_new", "Page 2"))
        assertEquals(listOf("Page 1", "Page 2"), grown.all().map { it.title })
        assertEquals(1, grown.books.size)
    }

    @Test
    fun deleting_moves_the_active_page_and_refuses_the_last_one() {
        val two = library(page("pg_a"), page("pg_b"), active = "pg_b")
        val left = two.withoutPage("pg_b")
        assertEquals(listOf("pg_a"), left.all().map { it.id })
        assertEquals("pg_a", left.active)
        assertEquals("the last page stays", 1, library(page("pg_a")).withoutPage("pg_a").all().size)
    }

    @Test
    fun renaming_replaces_only_the_named_page() {
        val two = library(page("pg_a", "First"), page("pg_b", "Second"))
        val renamed = two.withUpdated(page("pg_b", "Second, edited"))
        assertEquals("First", renamed.page("pg_a")!!.title)
        assertEquals("Second, edited", renamed.page("pg_b")!!.title)
    }

    @Test
    fun each_page_gets_its_own_stroke_file() {
        val dir = File(System.getProperty("java.io.tmpdir"), "penwise-pages-${System.nanoTime()}")
        val store = PageStore(dir)
        try {
            val a = store.strokes("pg_a")
            val b = store.strokes("pg_b")
            val stroke =
                SavedStroke(
                    0,
                    -0x1000000,
                    1f,
                    7L,
                    floatArrayOf(1f, 2f, 0.5f, 0f, 0f, 3f, 4f, 0.5f, 0f, 0f),
                    intArrayOf(0, 900),
                    2,
                )
            runBlocking {
                a.append(stroke)
                b.append(stroke)
                b.append(stroke)
                assertEquals(1, a.load().size)
                assertEquals(2, b.load().size)
            }
            assertTrue(File(dir, "pages/pg_a.bin").isFile)
        } finally {
            dir.deleteRecursively()
        }
    }

    @Test
    fun an_older_single_file_is_read_by_the_first_page() {
        val dir = File(System.getProperty("java.io.tmpdir"), "penwise-legacy-${System.nanoTime()}")
        try {
            val legacy = PageStore(dir).strokes(PageList.DEFAULT_PAGE_ID)
            val stroke =
                SavedStroke(
                    0,
                    -0x1000000,
                    1f,
                    9L,
                    floatArrayOf(1f, 2f, 0.5f, 0f, 0f, 3f, 4f, 0.5f, 0f, 0f),
                    intArrayOf(0, 900),
                    2,
                )
            runBlocking { legacy.append(stroke) }
            // Move the page file to the pre-pages location, as an upgrade would leave it.
            val pageFile = File(dir, "pages/${PageList.DEFAULT_PAGE_ID}.bin")
            assertTrue(pageFile.renameTo(File(dir, "page0.bin")))
            val store = PageStore(dir)
            store.load() // triggers the one-time migration
            val other = runBlocking { store.strokes("pg_other").load() }
            assertEquals("a new page does not inherit the old file", 0, other.size)
            val first = runBlocking { store.strokes(PageList.DEFAULT_PAGE_ID).load() }
            assertEquals("the first page reads it", 1, first.size)
        } finally {
            dir.deleteRecursively()
        }
    }

    @Test
    fun the_library_file_round_trips_and_falls_back_when_missing() {
        val dir = File(System.getProperty("java.io.tmpdir"), "penwise-meta-${System.nanoTime()}")
        try {
            val store = PageStore(dir)
            assertEquals(1, store.load().all().size)
            val grown = store.load().withPage(page("pg_x", "X"))
            store.save(grown)
            assertEquals(2, store.load().all().size)
            assertEquals("pg_x", store.load().page("pg_x")!!.id)
            assertTrue(store.newPageId().startsWith("pg_"))
            assertEquals(19, store.newPageId().length)
        } finally {
            dir.deleteRecursively()
        }
    }
}
