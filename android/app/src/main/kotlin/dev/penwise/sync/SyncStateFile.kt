package dev.penwise.sync

import java.io.File
import java.io.FileOutputStream
import java.io.IOException
import java.nio.channels.FileChannel
import java.nio.file.Files
import java.nio.file.StandardCopyOption
import java.nio.file.StandardOpenOption
import java.security.SecureRandom

private const val ID_BYTES = 16
private const val TIMESTAMP_BYTES = 6
private const val BYTE_BITS = 8

/**
 * The one durable file of the sync layer: the `SyncCore` state blob (clock, outbox, replica, server pin).
 * Written as temp file + fsync + atomic rename, so a crash leaves either the previous or the new state, never a
 * torn one.
 */
class SyncStateFile(
    private val dir: File,
    private val random: SecureRandom = SecureRandom(),
) {
    private val file = File(dir, NAME)

    /** The stored blob, or `null` when nothing was written yet. */
    fun read(): ByteArray? = file.takeIf(File::isFile)?.readBytes()

    /** Replaces the stored blob atomically. One writer at a time. */
    @Synchronized
    @Throws(IOException::class)
    fun write(state: ByteArray) {
        dir.mkdirs()
        val staging = File(dir, "$NAME.tmp")
        FileOutputStream(staging).use { out ->
            out.write(state)
            out.fd.sync()
        }
        Files.move(staging.toPath(), file.toPath(), StandardCopyOption.ATOMIC_MOVE, StandardCopyOption.REPLACE_EXISTING)
        // Make the rename itself durable; directories cannot always be opened, so this is best effort.
        runCatching { FileChannel.open(dir.toPath(), StandardOpenOption.READ).use { it.force(true) } }
    }

    /**
     * Opens the persisted core, or a fresh one with a new random device id. A blob that no longer decodes is kept
     * aside as `sync-state.bin.unreadable` (never deleted) and a fresh device starts.
     */
    fun open(clock: () -> Long = System::currentTimeMillis): SyncCore {
        val stored = read()
        if (stored != null) {
            runCatching { SyncCore.restore(stored, random.nextLong(), clock) }.onSuccess { return it }
            runCatching { file.copyTo(File(dir, "$NAME.unreadable"), overwrite = true) }
        }
        return SyncCore.create(newDeviceId(clock()), random.nextLong(), clock)
    }

    private fun newDeviceId(nowMs: Long): ByteArray {
        val id = ByteArray(ID_BYTES)
        random.nextBytes(id)
        // ULID layout: 48-bit big-endian millisecond timestamp, then 80 random bits.
        for (index in 0 until TIMESTAMP_BYTES) {
            id[index] = (nowMs ushr (BYTE_BITS * (TIMESTAMP_BYTES - 1 - index))).toByte()
        }
        return id
    }

    private companion object {
        const val NAME = "sync-state.bin"
    }
}
