package dev.penwise.sync

import dev.penwise.core.Native
import dev.penwise.ink.EditSink

private const val UPDATE_TITLE = 1
private const val UPDATE_POSITION = 2
private const val UPDATE_DELETED = 4
private const val DELETED_VALUE = 8
private const val UPDATE_BACKGROUND = 16

/** Client kind of the `Hello` frame: this app is a tablet. */
private const val KIND_TABLET = 0

private val EMPTY = ByteArray(0)

/**
 * Kotlin face of the Rust `SyncCore` (clock, outbox, replica, frame codec). Bytes in, bytes out; no rules here.
 *
 * Every native call on [handle] (and on any `PageScene` opened from it) must hold [lock]; the methods below do.
 * [onMutated] runs after every call that changed the replica or the outbox, still inside the lock's critical
 * section's aftermath (never while holding it), so the state file and the socket can follow.
 */
class SyncCore private constructor(
    val handle: Long,
    private val clock: () -> Long,
) : AutoCloseable,
    EditSink {
    /** Serializes every native call on [handle]. */
    val lock = Any()

    /** Called (on the calling thread, lock released) after local edits, receives and authoring. */
    @Volatile
    var onMutated: (() -> Unit)? = null
    private var closed = false

    private inline fun <T> guarded(block: () -> T): T =
        synchronized(lock) {
            check(!closed) { "SyncCore is closed" }
            block()
        }

    private fun mutated() {
        onMutated?.invoke()
    }

    /** This device's 16-byte id. */
    fun device(): ByteArray = guarded { Native.syncDevice(handle) }

    /** The `Hello` frame for a new connection. */
    fun hello(token: String): ByteArray = guarded { Native.syncHello(handle, token, KIND_TABLET, clock()) }

    /** Applies one server frame; the caller sends [Outcome.replies] and then polls [pollPush]. */
    fun receive(frame: ByteArray): Outcome {
        val outcome = guarded { Outcome.parse(Native.syncReceive(handle, frame, clock())) }
        if (outcome.mutated) mutated()
        return outcome
    }

    /** The next `PushOps` frame, or `null` when nothing may be sent now. */
    fun pollPush(): ByteArray? = guarded { Native.syncPollPush(handle) }.takeIf { it.isNotEmpty() }

    /** The socket is gone: nothing is in flight any more. */
    fun offline() = guarded { Native.syncOffline(handle) }

    /** Highest applied server sequence number. */
    fun headSeq(): Long = guarded { Native.syncHeadSeq(handle) }

    /** Ops waiting for an `Ack` / ops the server refused for good. */
    fun outbox(): OutboxCounts =
        guarded { Native.syncOutboxCounts(handle) }.let { OutboxCounts(pending = it[0], deadLetters = it[1]) }

    /** The notebook and page lists. */
    fun catalog(): Catalog = Catalog.parse(guarded { Native.syncCatalog(handle) })

    /** The persistable state blob (clock, outbox, replica, server pin). */
    fun exportState(): ByteArray = guarded { Native.syncExportState(handle) }

    /**
     * Like [exportState], but the lock is held only to clone the state; the postcard encoding runs outside it, so
     * the pen and render threads never wait on it.
     */
    fun exportStateOffLock(): ByteArray = Native.syncStateEncode(guarded { Native.syncStateBegin(handle) })

    /** Changes whenever the persistable state changed (the persister skips writes when it did not). */
    fun revision(): Long = guarded { Native.syncRevision(handle) }

    /** True once the replica holds everything the server had at its `Welcome` (or a snapshot ended). */
    fun caughtUp(): Boolean = guarded { Native.syncCaughtUp(handle) }

    /** Ops the server refused for good, oldest first. */
    fun deadLetters(): List<DeadLetter> = DeadLetter.parseAll(guarded { Native.syncDeadLetters(handle) })

    /** The stored bytes of one dead letter (for export); empty when unknown. */
    fun deadLetterBytes(opId: ByteArray): ByteArray = guarded { Native.syncDeadLetterBytes(handle, opId) }

    /** Forgets a dead letter once the user exported or dismissed it. */
    fun removeDeadLetter(opId: ByteArray) {
        guarded { Native.syncDeadLetterRemove(handle, opId) }
        mutated()
    }

    /** True while a `Welcome` named a different server and the core waits for [resolveServerChange]. */
    fun serverChangePending(): Boolean = guarded { Native.syncServerChangePending(handle) }

    /** Answers the "different server" question: `keep` re-sends the tablet's ink to it, otherwise it is dropped. */
    fun resolveServerChange(keep: Boolean): Outcome {
        val outcome = guarded { Outcome.parse(Native.syncResolveServerChange(handle, keep, clock())) }
        mutated()
        return outcome
    }

    /**
     * Runs [block] with the native handle while holding [lock], failing with `IllegalStateException` once the core
     * is closed: a scene opened from a stale handle would dereference freed memory.
     */
    fun <T> whileOpen(block: (handle: Long) -> T): T = guarded { block(handle) }

    /** A `PageScene` tool committed edits inside its native call: persist and push. */
    override fun committed() = mutated()

    /** `SwitchPage` frame: the browser follows the tablet. */
    fun switchPage(page: ByteArray): ByteArray = guarded { Native.syncSwitchPage(handle, page) }

    /** `StrokeDelta` preview frame. */
    @Suppress("LongParameterList")
    fun liveDelta(
        page: ByteArray,
        stroke: ByteArray,
        brush: IntArray,
        scale: Float,
        seed: Long,
        start: Int,
        samples: FloatArray,
        times: IntArray,
    ): ByteArray = guarded { Native.syncLiveDelta(handle, page, stroke, brush, scale, seed, start, samples, times) }

    /** `StrokeCancel` frame. */
    fun liveCancel(
        page: ByteArray,
        stroke: ByteArray,
    ): ByteArray = guarded { Native.syncLiveCancel(handle, page, stroke) }

    /** New notebook; returns its hex id. [id] pins a deterministic id (legacy import), `null` mints one. */
    fun createNotebook(
        title: String,
        position: Long,
        id: String? = null,
    ): String {
        val made = guarded { Native.syncCreateNotebook(handle, id?.let(Ids::bytes) ?: EMPTY, title, position, clock()) }
        mutated()
        return Ids.hex(made)
    }

    /** Renames and/or deletes a notebook. */
    fun updateNotebook(
        id: String,
        title: String? = null,
        deleted: Boolean? = null,
    ) {
        var flags = 0
        if (title != null) flags = flags or UPDATE_TITLE
        if (deleted != null) flags = flags or UPDATE_DELETED or (if (deleted) DELETED_VALUE else 0)
        val bytes = requireNotNull(Ids.bytes(id)) { "bad notebook id" }
        guarded { Native.syncUpdateNotebook(handle, bytes, flags, title.orEmpty(), 0L, clock()) }
        mutated()
    }

    /** New page in [notebook]; returns its hex id. [id] pins a deterministic id (legacy import). */
    @Suppress("LongParameterList")
    fun createPage(
        notebook: String,
        title: String,
        position: Long,
        size: FloatArray,
        background: Int,
        id: String? = null,
    ): String {
        val notebookId = requireNotNull(Ids.bytes(notebook)) { "bad notebook id" }
        val made =
            guarded {
                Native.syncCreatePage(
                    handle,
                    id?.let(Ids::bytes) ?: EMPTY,
                    notebookId,
                    title,
                    position,
                    size,
                    background,
                    clock(),
                )
            }
        mutated()
        return Ids.hex(made)
    }

    /** Retitles, repositions, re-backgrounds and/or deletes a page. */
    fun updatePage(
        id: String,
        title: String? = null,
        position: Long? = null,
        background: Int? = null,
        deleted: Boolean? = null,
    ) {
        var flags = 0
        if (title != null) flags = flags or UPDATE_TITLE
        if (position != null) flags = flags or UPDATE_POSITION
        if (background != null) flags = flags or UPDATE_BACKGROUND
        if (deleted != null) flags = flags or UPDATE_DELETED or (if (deleted) DELETED_VALUE else 0)
        val bytes = requireNotNull(Ids.bytes(id)) { "bad page id" }
        guarded {
            Native.syncUpdatePage(handle, bytes, flags, title.orEmpty(), position ?: 0L, background ?: 0, clock())
        }
        mutated()
    }

    /** One-time import of an old `PWST` page file. */
    fun importLegacyPage(
        pageText: String,
        file: ByteArray,
    ): ImportReport {
        val counts = guarded { Native.syncImportLegacyPage(handle, pageText, file, clock()) }
        mutated()
        return ImportReport(
            added = counts[0],
            alreadyPresent = counts[1],
            skipped = counts[2],
            seedCollisions = counts[3],
        )
    }

    override fun close() {
        synchronized(lock) {
            if (closed) return
            closed = true
            Native.syncFree(handle)
        }
    }

    companion object {
        /** Fresh state for `device` (16 bytes). */
        fun create(
            device: ByteArray,
            entropy: Long,
            clock: () -> Long = System::currentTimeMillis,
        ): SyncCore = SyncCore(Native.syncNew(device, entropy), clock)

        /** Rebuilds from [exportState]; throws `RuntimeException` for a corrupt blob. */
        fun restore(
            state: ByteArray,
            entropy: Long,
            clock: () -> Long = System::currentTimeMillis,
        ): SyncCore = SyncCore(Native.syncRestore(state, entropy), clock)

        /** Deterministic id (hex) of a legacy `nb_…` notebook. */
        fun legacyNotebookId(text: String): String = Ids.hex(Native.syncLegacyId(0, text))

        /** Deterministic id (hex) of a legacy `pg_…` page. */
        fun legacyPageId(text: String): String = Ids.hex(Native.syncLegacyId(1, text))
    }
}

/** Pending and dead-letter op counts. */
data class OutboxCounts(
    val pending: Int,
    val deadLetters: Int,
)

/** What a legacy page import did. */
data class ImportReport(
    val added: Int,
    val alreadyPresent: Int,
    val skipped: Int,
    /** Strokes that shared a seed with an earlier one in the file and got a distinct identity. */
    val seedCollisions: Int = 0,
)
