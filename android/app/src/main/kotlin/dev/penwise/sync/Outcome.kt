package dev.penwise.sync

import java.nio.ByteBuffer
import java.nio.ByteOrder

private const val OUTCOME_VERSION = 1
private const val ID_BYTES = 16
private const val FLAG_CATALOG = 1
private const val FLAG_RECONNECT = 2
private const val FLAG_SNAPSHOT_DONE = 4
private const val FLAG_PAGE = 8
private const val FLAG_SERVER_CHANGED = 16
private const val FLAG_FOCUS_ITEM = 32
private const val FLAG_FOCUS_SUPPORTED = 64
private const val CHANGE_BYTES = 2 * ID_BYTES

/** What one received server frame did (the blob layout is documented in `sync_core/outcome.rs`). */
class Outcome(
    /** Index of the `ServerMsg` variant: see [Kind]. */
    val kind: Int,
    /** Reject / Error reason code (see [Kind]); 0 otherwise. */
    val code: Int,
    val headSeq: Long,
    val catalogChanged: Boolean,
    /** The server identity changed: the replica was reset; close and `hello` again. */
    val reconnect: Boolean,
    val snapshotDone: Boolean,
    /** A `Welcome` named a different server; the core holds until [SyncCore.resolveServerChange]. */
    val serverChanged: Boolean,
    val applied: Int,
    val applyErrors: Int,
    val acked: Int,
    val rejected: Int,
    /** `Welcome.current_page`, `SwitchPage.page` or `Focus.page`. */
    val page: ByteArray?,
    /** `Focus.item`: the frame to fit; null with [Kind.FOCUS] means "fit the page's content". */
    val focusItem: ByteArray?,
    /** A `Welcome` from a server that speaks protocol 2 or newer, so `Focus` may be sent. */
    val focusSupported: Boolean,
    /** Frames to send back (Pong). */
    val replies: List<ByteArray>,
    /** The unparsed blob: `PageScene.refresh` takes it as is. */
    val raw: ByteArray,
) {
    /** True when the local replica or the outbox changed, so the state file is stale. */
    val mutated: Boolean
        get() =
            catalogChanged || snapshotDone || applied > 0 || acked > 0 || rejected > 0 || reconnect ||
                kind == Kind.OPS_SINCE || kind == Kind.WELCOME

    /** True when scenes may need a refresh. */
    val hasChanges: Boolean get() = applied > 0 || snapshotDone || reconnect

    companion object {
        /** Parses an outcome blob; throws `IllegalArgumentException` for a foreign one. */
        fun parse(raw: ByteArray): Outcome {
            val buffer = ByteBuffer.wrap(raw).order(ByteOrder.LITTLE_ENDIAN)
            require(buffer.get().toInt() == OUTCOME_VERSION) { "outcome version" }
            val kind = buffer.get().toInt() and 0xFF
            val flags = buffer.get().toInt() and 0xFF
            val code = buffer.get().toInt() and 0xFF
            val head = buffer.getLong()
            val applied = buffer.getInt()
            val errors = buffer.getInt()
            val acked = buffer.getInt()
            val rejected = buffer.getInt()
            val page = if (flags and FLAG_PAGE != 0) ByteArray(ID_BYTES).also(buffer::get) else null
            val replies = List(buffer.getInt()) { ByteArray(buffer.getInt()).also(buffer::get) }
            // The change list (page, item pairs) is read by the scene's own refresh; only its length matters here.
            val changes = buffer.getInt()
            buffer.position(buffer.position() + changes * CHANGE_BYTES)
            val focusItem = if (flags and FLAG_FOCUS_ITEM != 0) ByteArray(ID_BYTES).also(buffer::get) else null
            return Outcome(
                kind = kind,
                code = code,
                headSeq = head,
                catalogChanged = flags and FLAG_CATALOG != 0,
                reconnect = flags and FLAG_RECONNECT != 0,
                snapshotDone = flags and FLAG_SNAPSHOT_DONE != 0,
                serverChanged = flags and FLAG_SERVER_CHANGED != 0,
                applied = applied,
                applyErrors = errors,
                acked = acked,
                rejected = rejected,
                page = page,
                focusItem = focusItem,
                focusSupported = flags and FLAG_FOCUS_SUPPORTED != 0,
                replies = replies,
                raw = raw,
            )
        }
    }
}

/** `ServerMsg` variant indices and handshake reason codes. */
object Kind {
    const val WELCOME = 0
    const val REJECT = 1
    const val ACK = 2
    const val OPS_SINCE = 3
    const val SNAPSHOT_BEGIN = 4
    const val SNAPSHOT_CHUNK = 5
    const val SNAPSHOT_END = 6
    const val SWITCH_PAGE = 9

    /** `Focus`: switch to the page if needed, then fit the frame (or the page's content). */
    const val FOCUS = 14

    /** Frames of a catch-up (ops or snapshot): the stream of them ends the first sync. */
    val CATCH_UP = setOf(OPS_SINCE, SNAPSHOT_BEGIN, SNAPSHOT_CHUNK, SNAPSHOT_END)

    /** `Reject.code`: the token is wrong. */
    const val REJECT_BAD_TOKEN = 0
}
