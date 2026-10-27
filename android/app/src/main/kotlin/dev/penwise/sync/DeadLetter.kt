package dev.penwise.sync

import java.nio.ByteBuffer
import java.nio.ByteOrder

private const val DEAD_LETTER_VERSION = 1
private const val ID_BYTES = 16

/** One op the server refused for good (layout in `sync_core`'s dead-letter blob). */
class DeadLetter(
    /** The op's 16-byte id. */
    val opId: ByteArray,
    /** 0 item add, 1 item update, 2 item delete, 3 page, 4 notebook, 255 undecodable. */
    val kind: Int,
    /** The page it targeted, or `null` for catalog ops. */
    val page: ByteArray?,
    /** `OpRejection` index: see [reasonName]. */
    val reason: Int,
    val sizeBytes: Int,
) {
    val id: String get() = Ids.hex(opId)

    /** Short human label of the op kind. */
    val kindName: String
        get() = KIND_NAMES.getOrElse(kind) { "op" }

    /** Short human label of why the server refused it. */
    val reasonName: String
        get() = REASON_NAMES.getOrElse(reason) { "refused" }

    companion object {
        private val KIND_NAMES = listOf("stroke or item", "item edit", "item delete", "page", "notebook")
        private val REASON_NAMES =
            listOf(
                "not this device's op",
                "clock too far off",
                "invalid",
                "unknown page",
                "unknown notebook",
                "unknown item",
                "duplicate",
                "wrong page",
            )

        /** Parses the blob of `syncDeadLetters`; throws `IllegalArgumentException` for a foreign one. */
        fun parseAll(raw: ByteArray): List<DeadLetter> {
            val buffer = ByteBuffer.wrap(raw).order(ByteOrder.LITTLE_ENDIAN)
            require(buffer.get().toInt() == DEAD_LETTER_VERSION) { "dead letter version" }
            return List(buffer.getInt()) {
                val op = ByteArray(ID_BYTES).also(buffer::get)
                val kind = buffer.get().toInt() and 0xFF
                val page = ByteArray(ID_BYTES).also(buffer::get)
                val reason = buffer.get().toInt() and 0xFF
                DeadLetter(op, kind, page.takeIf { bytes -> bytes.any { it != 0.toByte() } }, reason, buffer.getInt())
            }
        }
    }
}
