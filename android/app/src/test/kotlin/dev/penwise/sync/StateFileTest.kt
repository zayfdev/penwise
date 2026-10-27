package dev.penwise.sync

import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertThrows
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
        val file = SyncStateFile(dir)
        val core = file.open()
        assertEquals(0, core.outbox().pending)
        assertEquals(1, file.recovered.size)
        assertTrue(
            file.recovered
                .single()
                .name
                .startsWith("sync-state.bin.unreadable"),
        )
        assertArrayEquals(byteArrayOf(9, 9, 9), file.recovered.single().readBytes())
        assertFalse("the live path is free for the new state", File(dir, "sync-state.bin").exists())
        core.close()
    }

    @Test
    fun unreadable_files_are_kept_under_unique_names_and_never_overwritten() {
        val dir = dir()
        val kept = ArrayList<File>()
        for (round in 1..3) {
            File(dir, "sync-state.bin").writeBytes(ByteArray(round) { round.toByte() })
            val file = SyncStateFile(dir)
            file.open { 1_700_000_000_000L }.close() // the same clock reading every time: names must still differ
            kept += file.recovered
        }
        assertEquals("one kept file per failure", 3, kept.size)
        assertEquals("all names differ", 3, kept.map(File::getName).toSet().size)
        for ((index, file) in kept.withIndex()) {
            assertArrayEquals(
                "round ${index + 1} is intact",
                ByteArray(index + 1) { (index + 1).toByte() },
                file.readBytes(),
            )
        }
    }

    @Test
    fun a_file_that_cannot_be_kept_aside_stops_the_start_instead_of_being_overwritten() {
        val dir = dir()
        File(dir, "sync-state.bin").writeBytes(byteArrayOf(9, 9, 9))
        dir.setWritable(false)
        try {
            if (dir.canWrite()) return // running as a user that ignores directory permissions
            assertThrows(java.io.IOException::class.java) { SyncStateFile(dir).open() }
        } finally {
            dir.setWritable(true)
        }
        assertArrayEquals(byteArrayOf(9, 9, 9), File(dir, "sync-state.bin").readBytes())
    }

    @Test
    fun the_persister_coalesces_requests_and_flush_waits_for_the_disk() {
        val dir = dir()
        val file = SyncStateFile(dir)
        val core = file.open()
        StatePersister(core, file).use { persister ->
            core.onMutated = persister::request
            repeat(20) { core.createNotebook("N$it", it.toLong()) }
            assertTrue(persister.flush())
            val restored = SyncStateFile(dir).open()
            assertEquals(20, restored.outbox().pending)
            restored.close()
        }
        assertNotNull(file.read())
        core.close()
    }

    /** A write sink that fails [failures] times with [error], then stores like the real file. */
    private class FlakySink(
        private val file: SyncStateFile,
        failures: Int,
        private val error: () -> Exception,
    ) : (ByteArray) -> Unit {
        private val left =
            java.util.concurrent.atomic
                .AtomicInteger(failures)
        val attempts =
            java.util.concurrent.atomic
                .AtomicInteger()

        override fun invoke(state: ByteArray) {
            attempts.incrementAndGet()
            if (left.getAndDecrement() > 0) throw error()
            file.write(state)
        }
    }

    @Test
    fun a_failing_disk_is_reported_retried_with_backoff_and_recovers_without_another_change() {
        val dir = dir()
        val file = SyncStateFile(dir)
        val core = file.open()
        val sink = FlakySink(file, failures = 3) { java.io.IOException("disk full") }
        val health = java.util.concurrent.CopyOnWriteArrayList<Boolean>()
        StatePersister(core, file, onFailing = { health.add(it) }, retryMs = longArrayOf(10, 20), write = sink).use {
            core.onMutated = it::request
            core.createNotebook("Kept", 0)
            eventually("the failure is visible") { it.failing }
            // Nothing else changes: the persister retries on its own until the disk takes the state.
            eventually("the write finally lands") { !it.failing }
            assertEquals(listOf(true, false), health.toList())
            assertEquals(4, sink.attempts.get())
        }
        val restored = SyncStateFile(dir).open()
        assertEquals(1, restored.outbox().pending)
        restored.close()
        core.close()
    }

    @Test
    fun flush_runs_its_own_write_and_says_whether_the_state_is_on_disk() {
        val dir = dir()
        val file = SyncStateFile(dir)
        val core = file.open()
        val sink = FlakySink(file, failures = 1) { java.io.IOException("disk full") }
        StatePersister(core, file, retryMs = longArrayOf(60_000), write = sink).use { persister ->
            core.onMutated = persister::request
            core.createNotebook("A", 0)
            eventually("first write failed") { persister.failing }
            assertEquals("the failed state is not on disk", null, file.read())
            assertTrue("flush writes it now and confirms", persister.flush())
            assertNotNull(file.read())
            assertFalse(persister.failing)
        }
        core.close()
    }

    @Test
    fun flush_reports_a_native_failure_instead_of_throwing() {
        val dir = dir()
        val file = SyncStateFile(dir)
        val core = file.open()
        val sink = FlakySink(file, failures = Int.MAX_VALUE) { IllegalStateException("native error") }
        StatePersister(core, file, retryMs = longArrayOf(60_000), write = sink).use { persister ->
            core.createNotebook("A", 0)
            persister.request()
            assertFalse("a non-IO failure is a false, not an ExecutionException", persister.flush())
            assertTrue(persister.failing)
        }
        core.close()
    }

    @Test
    fun the_off_lock_export_is_the_same_state_and_never_blocks_the_core_while_encoding() {
        val core = SyncStateFile(dir()).open()
        repeat(50) { core.createNotebook("N$it", it.toLong()) }
        assertArrayEquals(core.exportState(), core.exportStateOffLock())
        val restored = SyncCore.restore(core.exportStateOffLock(), 1L)
        assertEquals(50, restored.outbox().pending)
        restored.close()
        core.close()
    }
}
