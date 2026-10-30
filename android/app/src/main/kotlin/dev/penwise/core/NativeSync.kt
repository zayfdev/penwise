package dev.penwise.core

/**
 * JNI surface of the sync core (`jni_sync`, `jni_sync_state`, `jni_sync_authoring`): see
 * `crates/penwise-ffi/src/sync_core` for the blob layouts. Wrapped by `SyncCore`.
 */
@Suppress("TooManyFunctions") // one object per JNI class: the symbol names pin it
internal object NativeSync {
    init {
        System.loadLibrary("penwise_ffi")
    }

    external fun syncNew(
        device: ByteArray,
        entropy: Long,
    ): Long

    external fun syncRestore(
        state: ByteArray,
        entropy: Long,
    ): Long

    external fun syncFree(handle: Long)

    external fun syncExportState(handle: Long): ByteArray

    /** True once the replica has everything the server had at `Welcome` (or a snapshot ended); false before. */
    external fun syncCaughtUp(handle: Long): Boolean

    /** True while a `Welcome` named a different server and the core waits for [syncResolveServerChange]. */
    external fun syncServerChangePending(handle: Long): Boolean

    /** `keep`: re-queue every item for the new server; otherwise drop acked ink. Returns an `Outcome` blob. */
    external fun syncResolveServerChange(
        handle: Long,
        keep: Boolean,
        nowMs: Long,
    ): ByteArray

    /** Changes whenever the persistable state changed; equal values mean nothing new to write. */
    external fun syncRevision(handle: Long): Long

    /** Clones the persistable state (call under the core lock); returns a snapshot handle for [syncStateEncode]. */
    external fun syncStateBegin(handle: Long): Long

    /** Encodes and frees a [syncStateBegin] snapshot; needs no lock. Call exactly once per snapshot. */
    external fun syncStateEncode(snapshot: Long): ByteArray

    /** Frees a [syncStateBegin] snapshot without encoding it. */
    external fun syncStateDiscard(snapshot: Long)

    /** Dead-letter list blob (layout in `sync_core/dead_letters`). */
    external fun syncDeadLetters(handle: Long): ByteArray

    /** The stored envelope bytes of one dead letter; empty when unknown. */
    external fun syncDeadLetterBytes(
        handle: Long,
        opId: ByteArray,
    ): ByteArray

    /** Drops one dead letter. */
    external fun syncDeadLetterRemove(
        handle: Long,
        opId: ByteArray,
    )

    external fun syncDevice(handle: Long): ByteArray

    /** `kind`: 0 tablet, 1 browser, 2 agent. Returns the `Hello` frame. */
    external fun syncHello(
        handle: Long,
        token: String,
        kind: Int,
        nowMs: Long,
    ): ByteArray

    /** One server frame in, the `Outcome` blob out. */
    external fun syncReceive(
        handle: Long,
        frame: ByteArray,
        nowMs: Long,
    ): ByteArray

    /** The next `PushOps` frame, empty when nothing may be sent now. */
    external fun syncPollPush(handle: Long): ByteArray

    external fun syncOffline(handle: Long)

    external fun syncHeadSeq(handle: Long): Long

    /** `[pending ops, dead letters]`. */
    external fun syncOutboxCounts(handle: Long): IntArray

    external fun syncCatalog(handle: Long): ByteArray

    /** Stamps and queues an `encode_edits` blob for [page]; returns the op count. */
    external fun syncCommitEdits(
        handle: Long,
        page: ByteArray,
        edits: ByteArray,
        nowMs: Long,
    ): Int

    external fun syncSwitchPage(
        handle: Long,
        page: ByteArray,
    ): ByteArray

    /**
     * `Focus` frame: ask the other devices to show [page] and fit frame [item] (empty = fit the page's content).
     * Empty when the server's `Welcome` did not report protocol 2.
     */
    external fun syncFocus(
        handle: Long,
        page: ByteArray,
        item: ByteArray,
    ): ByteArray

    /** True when this connection's `Welcome` reported protocol 2 or newer, so [syncFocus] may be sent. */
    external fun syncServerSupportsFocus(handle: Long): Boolean

    /** `Watch` frame; an empty [page] stops watching. */
    external fun syncWatch(
        handle: Long,
        page: ByteArray,
    ): ByteArray

    external fun syncPing(
        handle: Long,
        nonce: Long,
    ): ByteArray

    /** An empty [id] mints one; returns the id. */
    external fun syncCreateNotebook(
        handle: Long,
        id: ByteArray,
        title: String,
        position: Long,
        nowMs: Long,
    ): ByteArray

    /** Flags: 1 title, 2 position, 4 deleted (value in 8). */
    external fun syncUpdateNotebook(
        handle: Long,
        id: ByteArray,
        flags: Int,
        title: String,
        position: Long,
        nowMs: Long,
    )

    /** Empty [id] mints one; empty [size] = infinite else `[width, height]`; returns the id. */
    @Suppress("LongParameterList")
    external fun syncCreatePage(
        handle: Long,
        id: ByteArray,
        notebook: ByteArray,
        title: String,
        position: Long,
        size: FloatArray,
        background: Int,
        nowMs: Long,
    ): ByteArray

    /** Flags: 1 title, 2 position, 4 deleted (value in 8), 16 background. */
    @Suppress("LongParameterList")
    external fun syncUpdatePage(
        handle: Long,
        id: ByteArray,
        flags: Int,
        title: String,
        position: Long,
        background: Int,
        nowMs: Long,
    )

    /** Imports an old `PWST` page file; returns `[added, alreadyPresent, skipped, seedCollisions]`. */
    external fun syncImportLegacyPage(
        handle: Long,
        pageText: String,
        file: ByteArray,
        nowMs: Long,
    ): IntArray

    /** `StrokeDelta` frame for `count` samples; [start] is the index of the first. */
    @Suppress("LongParameterList")
    external fun syncLiveDelta(
        handle: Long,
        page: ByteArray,
        stroke: ByteArray,
        brush: IntArray,
        scale: Float,
        seed: Long,
        start: Int,
        samples: FloatArray,
        times: IntArray,
    ): ByteArray

    external fun syncLiveCancel(
        handle: Long,
        page: ByteArray,
        stroke: ByteArray,
    ): ByteArray

    /** Deterministic legacy id: [kind] 0 notebook, 1 page; static, no handle. */
    external fun syncLegacyId(
        kind: Int,
        text: String,
    ): ByteArray
}
