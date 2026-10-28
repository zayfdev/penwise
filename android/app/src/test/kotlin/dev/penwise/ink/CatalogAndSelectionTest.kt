package dev.penwise.ink

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class CatalogAndSelectionTest {
    private fun list(
        active: String,
        vararg ids: String,
    ) = PageList(active, listOf(PageBook("b", "Book", ids.map { PageMeta(it, it, false, "blank") })))

    @Test
    fun a_catalog_republish_keeps_the_page_the_user_picked() {
        // The catalog was read while A was active; the user tapped B before it was published.
        val fresh = list("A", "A", "B")
        assertEquals("B", fresh.withPreferred("B").active)
    }

    @Test
    fun a_picked_page_that_is_gone_falls_back_to_the_catalogs_choice() {
        val fresh = list("A", "A", "B")
        assertEquals("A", fresh.withPreferred("deleted").active)
        assertEquals("A", fresh.withPreferred(null).active)
    }

    @Test
    fun the_lasso_grabs_a_selection_it_misses_by_a_finger_width() {
        // A 3 px underline at 0.5x: the pen lands 10 screen px (20 page px) below it.
        assertTrue(hitsSelection(0f, 100f, 200f, 103f, 50f, 122f, 0.5f))
        assertTrue("inflated by about 20 screen px", hitsSelection(0f, 0f, 10f, 10f, 10f + 19f, 5f, 1f))
        assertFalse("but not by a hand's width", hitsSelection(0f, 0f, 10f, 10f, 10f + 60f, 5f, 1f))
    }

    @Test
    fun the_slop_shrinks_in_page_units_as_the_view_zooms_in() {
        assertFalse(hitsSelection(0f, 0f, 10f, 10f, 10f + 6f, 5f, 4f))
        assertTrue(hitsSelection(0f, 0f, 10f, 10f, 10f + 4f, 5f, 4f))
    }

    @Test
    fun clearing_a_selection_wipes_its_frame_but_clearing_nothing_does_not() {
        val selection = Selection()
        var wiped = 0
        selection.onCleared = { wiped++ }
        selection.clear()
        assertEquals(0, wiped)
        selection.select(ByteArray(ItemKey.SIZE))
        selection.clear()
        assertEquals(1, wiped)
        assertEquals(0, selection.count)
    }
}
