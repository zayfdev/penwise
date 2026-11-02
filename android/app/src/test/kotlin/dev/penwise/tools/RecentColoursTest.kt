package dev.penwise.tools

import org.junit.Assert.assertEquals
import org.junit.Test

class RecentColoursTest {
    @Test
    fun a_new_pick_goes_to_the_front() {
        val recents = RecentColours(start = listOf(0x111111, 0x222222), limit = 3)
        recents.remember(0x333333)
        assertEquals(listOf(0x333333, 0x111111, 0x222222), recents.items)
    }

    @Test
    fun a_repeated_pick_moves_up_instead_of_doubling() {
        val recents = RecentColours(start = listOf(0x111111, 0x222222, 0x333333), limit = 3)
        recents.remember(0x333333)
        assertEquals(listOf(0x333333, 0x111111, 0x222222), recents.items)
    }

    @Test
    fun the_oldest_pick_falls_off() {
        val recents = RecentColours(start = listOf(0x111111, 0x222222), limit = 2)
        recents.remember(0x444444)
        assertEquals(listOf(0x444444, 0x111111), recents.items)
    }

    @Test
    fun the_pinned_colours_never_enter_the_list() {
        val recents = RecentColours(start = listOf(0x111111), limit = 3)
        Palette.pinned.forEach { recents.remember(it) }
        assertEquals(listOf(0x111111), recents.items)
    }

    @Test
    fun alpha_does_not_make_a_second_entry() {
        val recents = RecentColours(start = listOf(0x111111), limit = 3)
        recents.remember(0xFF111111.toInt())
        assertEquals(listOf(0x111111), recents.items)
    }

    @Test
    fun the_starter_list_is_plain_rgb() {
        assertEquals(Palette.starterRecents.map { it and 0xFFFFFF }, RecentColours().items)
    }

    @Test
    fun ink_black_and_graphite_have_their_own_buttons_and_never_enter() {
        val recents = RecentColours(start = listOf(0x111111), limit = 3)
        Palette.inkPins.forEach { recents.remember(it) }
        assertEquals(listOf(0x111111), recents.items)
    }

    @Test
    fun every_new_list_goes_to_the_saver() {
        val saved = mutableListOf<List<Int>>()
        val recents = RecentColours(start = listOf(0x111111), limit = 3) { saved.add(it) }
        recents.remember(0x222222)
        assertEquals(listOf(listOf(0x222222, 0x111111)), saved)
    }

    @Test
    fun the_saved_text_reads_back_to_the_same_list() {
        val items = listOf(0x123456, 0x00000A, 0xFEDCBA)
        assertEquals("123456,00000A,FEDCBA", RecentColours.encode(items))
        assertEquals(items, RecentColours.decode(RecentColours.encode(items)))
    }

    @Test
    fun unreadable_or_missing_saved_text_is_skipped() {
        assertEquals(null, RecentColours.decode(null))
        assertEquals(null, RecentColours.decode("  "))
        assertEquals(listOf(0x123456), RecentColours.decode("zzzz,123456, "))
    }
}
