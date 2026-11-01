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

    // Pairs printed by penwise-proto's own PageId (`ids.rs`): from_parts(0x010203040506, 0x0A0B..1213), the ULID
    // spec's example text, and the all-ones id.
    private val server =
        listOf(
            "0102030405060a0b0c0d0e0f10111213" to "01081G8186185GR38E1W8124GK",
            "01563e3ab5d3d6764c61efb99302bd5b" to "01ARZ3NDEKTSV4RRFFQ69G5FAV",
            "ffffffffffffffffffffffffffffffff" to "7ZZZZZZZZZZZZZZZZZZZZZZZZZ",
            "00000000000000000000000000000000" to "00000000000000000000000000",
        )

    @Test
    fun a_hex_id_becomes_the_servers_ulid() {
        for ((hex, ulid) in server) assertEquals(ulid, Ids.ulid(hex))
    }

    @Test
    fun a_server_ulid_becomes_the_hex_id() {
        for ((hex, ulid) in server) {
            assertEquals(hex, Ids.fromUlid(ulid))
            assertEquals(hex, Ids.fromUlid(ulid.lowercase()))
        }
    }

    @Test
    fun ulid_aliases_and_bad_text_follow_the_server() {
        assertEquals(Ids.fromUlid("00000000000000000000000001"), Ids.fromUlid("0000000000000000000000000l"))
        assertEquals(Ids.fromUlid("00000000000000000000000000"), Ids.fromUlid("0000000000000000000000000O"))
        assertNull(Ids.fromUlid("80000000000000000000000000"))
        assertNull(Ids.fromUlid("0000000000000000000000000"))
        assertNull(Ids.fromUlid("0000000000000000000000000U"))
        assertNull(Ids.ulid("p1"))
    }
}
