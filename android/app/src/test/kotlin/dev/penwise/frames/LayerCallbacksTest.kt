package dev.penwise.frames

import org.junit.Assert.assertEquals
import org.junit.Test

/** The first lock and the first live frame reach a listener whichever order the stage wires things in. */
class LayerCallbacksTest {
    private val callbacks = LayerCallbacks()

    @Test
    fun a_lock_announced_before_the_listener_is_set_is_replayed_to_it() {
        callbacks.lockChanged("og")
        val seen = ArrayList<String?>()
        callbacks.onLocked = { seen += it }
        assertEquals(listOf<String?>("og"), seen)
        callbacks.lockChanged("a")
        assertEquals(listOf<String?>("og", "a"), seen)
    }

    @Test
    fun the_first_live_frame_is_replayed_the_same_way() {
        callbacks.focusChanged("og")
        val seen = ArrayList<String?>()
        callbacks.onFocused = { seen += it }
        assertEquals(listOf<String?>("og"), seen)
    }

    @Test
    fun a_listener_set_before_any_change_hears_nothing_until_one_happens() {
        val seen = ArrayList<String?>()
        callbacks.onLocked = { seen += it }
        assertEquals(emptyList<String?>(), seen)
        callbacks.lockChanged("og")
        callbacks.lockChanged(null)
        assertEquals(listOf<String?>("og", null), seen)
    }
}
