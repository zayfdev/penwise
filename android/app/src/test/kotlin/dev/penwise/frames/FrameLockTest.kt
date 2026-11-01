package dev.penwise.frames

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** Which frame is locked: the opening pick, a lane's live frame taking over a lost one. */
class FrameLockTest {
    private fun frame(
        id: String,
        x: Float,
        y: Float = 0f,
        route: String = "/",
        group: String? = null,
        label: String = "",
    ) = FrameSpec(id, "p", route, "desktop", "http://x/", x, y, 1440, 900, group = group, label = label)

    private val lock = FrameLock()

    @Test
    fun a_board_without_a_lock_opens_on_the_top_lanes_plain_frame() {
        val specs =
            listOf(frame("about", 0f, 9000f, "/about"), frame("home", 100f, 0f), frame("v", 0f, -5f, group = "g"))
        assertTrue(lock.reconcile(specs))
        assertEquals("home", lock.locked)
    }

    @Test
    fun locking_another_frame_changes_the_lock_once() {
        val specs = listOf(frame("og", 0f), frame("a", 1500f, group = "g", label = "A"))
        lock.reconcile(specs)
        assertTrue(lock.lockTo(specs[1]))
        assertEquals("a", lock.locked)
        assertFalse(lock.lockTo(specs[1]))
    }

    @Test
    fun a_lock_that_is_still_there_is_left_alone() {
        val specs = listOf(frame("home", 0f), frame("other", 100f, 5000f, "/x"))
        lock.reconcile(specs)
        lock.lockTo(specs[1])
        assertFalse(lock.reconcile(specs))
        assertEquals("other", lock.locked)
    }

    @Test
    fun a_deleted_variant_hands_the_lock_to_its_lanes_live_frame() {
        val before = listOf(frame("home", 0f), frame("a", 1500f, group = "g", label = "A"), frame("x", 0f, 5000f, "/x"))
        lock.reconcile(before)
        lock.lockTo(before[1])
        // The round resolved: the variants are gone.
        assertTrue(lock.reconcile(listOf(before[0], before[2])))
        assertEquals("home", lock.locked)
    }

    @Test
    fun a_lane_whose_live_frame_moved_right_is_followed() {
        val before = listOf(frame("old", 0f), frame("x", 0f, 5000f, "/x"))
        lock.reconcile(before)
        assertTrue(lock.reconcile(listOf(frame("new", 3000f), before[1])))
        assertEquals("new", lock.locked)
    }

    @Test
    fun a_wanted_frame_wins_once_it_exists() {
        val first = listOf(frame("home", 0f))
        lock.reconcile(first)
        lock.want("later")
        assertFalse("not known yet", lock.reconcile(first))
        assertEquals("home", lock.locked)
        assertTrue(lock.reconcile(first + frame("later", 100f, 5000f, "/x")))
        assertEquals("later", lock.locked)
    }

    @Test
    fun clearing_forgets_a_wanted_frame_too() {
        lock.want("later")
        lock.clear()
        assertTrue(lock.reconcile(listOf(frame("home", 0f), frame("later", 100f, 5000f, "/x"))))
        assertEquals("home", lock.locked)
    }

    @Test
    fun a_page_without_frames_has_no_lock() {
        lock.reconcile(listOf(frame("home", 0f)))
        assertFalse(lock.reconcile(emptyList()))
        assertNull(lock.locked)
    }
}
