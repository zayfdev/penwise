package dev.penwise.frames

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Test

/** One live page, one warm, nothing older (contract §3). */
class PageSlotsTest {
    private class Page(
        val id: String,
    ) {
        var state = "new"
    }

    private val log = ArrayList<String>()
    private val slots =
        PageSlots<Page>(
            idOf = { it.id },
            open = { spec -> Page(spec.id).also { log += "open ${spec.id}" } },
            wake = {
                it.state = "live"
                log += "wake ${it.id}"
            },
            park = {
                it.state = "warm"
                log += "park ${it.id}"
            },
            destroy = {
                it.state = "destroyed"
                log += "destroy ${it.id}"
            },
        )

    private fun spec(id: String) = FrameSpec(id, "p", "/", "desktop", "http://x/", 0f, 0f, 1440, 900)

    @Test
    fun the_first_focus_opens_a_page_and_none_means_nothing_is_live() {
        assertNull(slots.focus(spec("a")))
        assertEquals("a", slots.live?.id)
        assertNull(slots.warm)
        assertEquals(listOf("open a"), log)
        assertEquals(slots.live, slots.focus(null))
        assertNull(slots.live)
        assertEquals("a", slots.warm?.id)
    }

    @Test
    fun the_page_that_was_live_is_parked_warm_and_the_one_before_it_is_destroyed() {
        slots.focus(spec("a"))
        val a = requireNotNull(slots.live)
        slots.focus(spec("b"))
        val b = requireNotNull(slots.live)
        assertSame(a, slots.warm)
        slots.focus(spec("c"))
        assertEquals("destroyed", a.state)
        assertSame(b, slots.warm)
        assertEquals("warm", b.state)
        assertEquals(listOf("open a", "open b", "park a", "open c", "destroy a", "park b"), log)
    }

    @Test
    fun a_warm_page_comes_back_woken_not_reopened() {
        slots.focus(spec("a"))
        val a = requireNotNull(slots.live)
        slots.focus(spec("b"))
        log.clear()
        slots.focus(spec("a"))
        assertSame(a, slots.live)
        assertEquals("b", slots.warm?.id)
        assertEquals(listOf("wake a", "park b"), log)
        assertEquals("live", a.state)
    }

    @Test
    fun focusing_the_live_page_again_changes_nothing() {
        slots.focus(spec("a"))
        log.clear()
        assertNull(slots.focus(spec("a")))
        assertEquals(emptyList<String>(), log)
    }

    @Test
    fun closing_a_frame_destroys_its_page_wherever_it_is() {
        slots.focus(spec("a"))
        slots.focus(spec("b"))
        slots.close("a")
        assertNull(slots.warm)
        slots.close("b")
        assertNull(slots.live)
        assertEquals(listOf("open a", "open b", "park a", "destroy a", "destroy b"), log)
    }

    @Test
    fun destroying_everything_leaves_no_page() {
        slots.focus(spec("a"))
        slots.focus(spec("b"))
        slots.destroyAll()
        assertEquals(emptyList<Page>(), slots.all())
        assertEquals(2, log.count { it.startsWith("destroy") })
    }
}
