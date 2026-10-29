package dev.penwise.frames

import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The ghost/variant patch decisions (P3): what goes to the page after a load, when the patch changes, and when the
 * owner toggles the Ghost chip. The page forgets the patch on every load, so a reset must re-send it.
 */
class PatchFeedTest {
    private val patch = """{"css":"h1{color:red}","dom":[]}"""

    private fun message(
        feed: PatchFeed,
        patch: String?,
        on: Boolean = true,
    ): JSONObject? = feed.next(patch, on)

    @Test
    fun a_frame_without_a_patch_sends_nothing() {
        assertNull(message(PatchFeed(), null))
    }

    @Test
    fun a_patch_is_sent_once_and_applied() {
        val feed = PatchFeed()
        val sent = message(feed, patch)
        assertEquals("pw:patch", sent?.optString("t"))
        assertEquals("h1{color:red}", sent?.optJSONObject("patch")?.optString("css"))
        assertTrue(sent?.optBoolean("on") == true)
        assertNull("the page already shows it", message(feed, patch))
    }

    @Test
    fun a_changed_patch_is_sent_again() {
        val feed = PatchFeed()
        message(feed, patch)
        val changed = """{"css":"h1{color:blue}","dom":[]}"""
        val sent = message(feed, changed)
        assertEquals("h1{color:blue}", sent?.optJSONObject("patch")?.optString("css"))
    }

    @Test
    fun the_toggle_sends_the_revert_and_the_reapply() {
        val feed = PatchFeed()
        message(feed, patch)
        val off = message(feed, patch, on = false)
        assertTrue(off?.optBoolean("on") == false)
        assertNull("already reverted", message(feed, patch, on = false))
        val on = message(feed, patch, on = true)
        assertTrue(on?.optBoolean("on") == true)
    }

    @Test
    fun a_cleared_patch_reverts_what_was_applied() {
        val feed = PatchFeed()
        message(feed, patch)
        val revert = message(feed, null)
        assertTrue(revert?.optBoolean("on") == false)
        assertNull("nothing left to revert", message(feed, null))
    }

    @Test
    fun a_load_reapplies_the_patch() {
        val feed = PatchFeed()
        message(feed, patch)
        feed.reset()
        val again = message(feed, patch)
        assertTrue("a fresh document needs the patch again", again != null)
        assertTrue(again?.optBoolean("on") == true)
    }

    @Test
    fun a_load_does_not_reapply_a_reverted_patch() {
        val feed = PatchFeed()
        feed.reset()
        assertNull(message(feed, patch, on = false))
    }

    @Test
    fun a_patch_that_is_not_json_is_never_sent() {
        assertNull(message(PatchFeed(), "not a patch"))
    }
}
