package dev.penwise.sync

import java.io.DataOutputStream
import java.io.OutputStream

private val DEAD_LETTER_MAGIC = "PWDL".toByteArray()
private val STATE_MAGIC = "PWSB".toByteArray()
private const val EXPORT_VERSION = 1

/**
 * Plain files the user can keep when the sync layer cannot hold on to something: the refused ops and the whole
 * state. Both are binary and exactly what the core stores, so a later tool can re-import them.
 */
object Exports {
    /** `PWDL`, version, count, then per op: 16-byte id, u32 length, the stored envelope. Returns the op count. */
    fun deadLetters(
        core: SyncCore,
        out: OutputStream,
    ): Int {
        val letters = core.deadLetters()
        val stream = DataOutputStream(out)
        stream.write(DEAD_LETTER_MAGIC)
        stream.writeByte(EXPORT_VERSION)
        stream.writeInt(letters.size)
        for (letter in letters) {
            val bytes = core.deadLetterBytes(letter.opId)
            stream.write(letter.opId)
            stream.writeInt(bytes.size)
            stream.write(bytes)
        }
        stream.flush()
        return letters.size
    }

    /** `PWSB`, version, then the state blob as the state file holds it. */
    fun state(
        core: SyncCore,
        out: OutputStream,
    ) {
        val stream = DataOutputStream(out)
        stream.write(STATE_MAGIC)
        stream.writeByte(EXPORT_VERSION)
        stream.write(core.exportStateOffLock())
        stream.flush()
    }
}
