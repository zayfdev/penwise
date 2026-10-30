package dev.penwise.sync

import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * The outcome blob of a `Focus` and of a protocol-2 `Welcome`, read from the files the Rust encoder wrote
 * (`crates/penwise-ffi/tests/fixtures/outcome_*.bin`, checked by `tests/outcome_fixture.rs`): the two layouts cannot
 * drift apart without one of the two tests failing.
 */
class OutcomeFocusTest {
    private fun fixture(name: String): ByteArray {
        val moduleDir = File(System.getProperty("user.dir").orEmpty())
        val file = File(moduleDir, "../../crates/penwise-ffi/tests/fixtures/$name").canonicalFile
        return file.readBytes()
    }

    private fun id(base: Int) = ByteArray(16) { (base + it).toByte() }

    @Test
    fun a_focus_carries_its_page_and_the_frame_to_fit_after_replies_and_changes() {
        val outcome = Outcome.parse(fixture("outcome_focus_item.bin"))
        assertEquals(Kind.FOCUS, outcome.kind)
        assertEquals(0x0102030405060708L, outcome.headSeq)
        assertEquals(3, outcome.applied)
        assertArrayEquals(id(0x10), outcome.page)
        val replies = outcome.replies.map { frame -> frame.map { it.toInt() and 0xFF } }
        assertEquals(listOf(listOf(0xA1, 0xA2, 0xA3), listOf(0xB1)), replies)
        assertArrayEquals(id(0x70), outcome.focusItem)
    }

    @Test
    fun a_focus_without_an_item_means_the_pages_content() {
        val outcome = Outcome.parse(fixture("outcome_focus_page.bin"))
        assertEquals(Kind.FOCUS, outcome.kind)
        assertNull(outcome.focusItem)
        assertArrayEquals(id(0x10), outcome.page)
    }

    @Test
    fun a_protocol_2_welcome_says_focus_may_be_sent() {
        val welcome = Outcome.parse(fixture("outcome_welcome_focus.bin"))
        assertEquals(Kind.WELCOME, welcome.kind)
        assertTrue(welcome.focusSupported)
        assertFalse(Outcome.parse(fixture("outcome_focus_page.bin")).focusSupported)
    }

    @Test
    fun focus_is_not_sent_to_a_server_that_has_not_said_it_understands_it() {
        SyncCore.create(ByteArray(16) { it.toByte() }.also { it[0] = 0 }, 1L).use { core ->
            assertFalse(core.supportsFocus())
            assertNull(core.focus(ByteArray(16) { 1 }, ByteArray(16) { 2 }))
        }
    }
}
