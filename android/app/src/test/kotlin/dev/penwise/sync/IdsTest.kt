package dev.penwise.sync

import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class IdsTest {
    @Test
    fun hex_round_trips_every_byte_value() {
        val bytes = ByteArray(16) { (it * 17 + 3).toByte() }
        val text = Ids.hex(bytes)
        assertEquals(32, text.length)
        assertArrayEquals(bytes, Ids.bytes(text))
        assertArrayEquals(bytes, Ids.bytes(text.uppercase()))
    }

    @Test
    fun malformed_ids_are_rejected() {
        assertNull(Ids.bytes(""))
        assertNull(Ids.bytes("abc"))
        assertNull(Ids.bytes("z".repeat(32)))
    }

    @Test
    fun legacy_ids_are_deterministic_and_distinct_per_kind() {
        assertEquals(SyncCore.legacyPageId("pg_a"), SyncCore.legacyPageId("pg_a"))
        assertEquals(32, SyncCore.legacyPageId("pg_a").length)
        assert(SyncCore.legacyPageId("pg_a") != SyncCore.legacyNotebookId("pg_a"))
    }
}
