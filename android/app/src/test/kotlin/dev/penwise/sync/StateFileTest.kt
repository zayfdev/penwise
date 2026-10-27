package dev.penwise.sync

import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File
import java.nio.file.Files

/** The durable file: atomic replacement, restore, and what happens to a blob nobody can read. */
class StateFileTest {
    private fun dir() = Files.createTempDirectory("penwise-state").toFile()

    @Test
    fun the_state_round_trips_with_the_same_device_and_outbox() {
        val dir = dir()
        val file = SyncStateFile(dir)
        val first = file.open()
        first.createNotebook("Kept", 0)
        val device = first.device()
        file.write(first.exportState())
        first.close()

        assertFalse("no staging file is left behind", File(dir, "sync-state.bin.tmp").exists())
        val second = file.open()
        assertArrayEquals(device, second.device())
        assertEquals(1, second.outbox().pending)
        assertEquals(listOf("Kept"), second.catalog().liveNotebooks.map { it.title })
        second.close()
    }

    @Test
    fun a_first_run_has_no_file_and_gets_a_ulid_shaped_device_id() {
        val dir = dir()
        val file = SyncStateFile(dir)
        assertNull(file.read())
        val core = file.open { 1_700_000_000_000L }
        val id = core.device()
        assertEquals(16, id.size)
        val timestamp = id.take(6).fold(0L) { acc, byte -> (acc shl 8) or (byte.toLong() and 0xFF) }
        assertEquals(1_700_000_000_000L, timestamp)
        core.close()
    }

    @Test
    fun an_unreadable_blob_is_kept_aside_and_a_fresh_core_starts() {
        val dir = dir()
        File(dir, "sync-state.bin").writeBytes(byteArrayOf(9, 9, 9))
        val core = SyncStateFile(dir).open()
        assertEquals(0, core.outbox().pending)
        assertTrue(File(dir, "sync-state.bin.unreadable").isFile)
        core.close()
    }

    @Test
    fun the_persister_coalesces_requests_and_flush_waits_for_the_disk() {
        val dir = dir()
        val file = SyncStateFile(dir)
        val core = file.open()
        StatePersister(core, file).use { persister ->
            core.onMutated = persister::request
            repeat(20) { core.createNotebook("N$it", it.toLong()) }
            persister.flush()
            val restored = SyncStateFile(dir).open()
            assertEquals(20, restored.outbox().pending)
            restored.close()
        }
        assertNotNull(file.read())
        core.close()
    }
}
