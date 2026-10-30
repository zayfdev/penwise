package dev.penwise.core

/**
 * JNI surface of `crates/penwise-ffi` (`libpenwise_ffi.so`). Wrapped by [InkStroke], [InkPresets],
 * `PageScene` and the sync core wrapper; nothing else should call it. Native errors and panics surface as
 * `RuntimeException`.
 */
@Suppress("TooManyFunctions") // one object per JNI class: the symbol names pin it
internal object Native {
    init {
        System.loadLibrary("penwise_ffi")
    }

    /** Returns a stroke handle; free it with [strokeFree]. */
    external fun strokeNew(
        preset: Int,
        seed: Long,
        argb: Int,
        sizeScale: Float,
    ): Long

    external fun strokeFree(handle: Long)

    /** Empties the stroke (same brush and seed) so the handle and its buffers are reused. */
    external fun strokeReset(handle: Long)

    /** `[isStamps, opacity, multiply]`, booleans as 0/1. */
    external fun strokeStyle(handle: Long): FloatArray

    external fun strokePush(
        handle: Long,
        samples: FloatArray,
        tUs: IntArray,
        count: Int,
    )

    /** Outline points (x,y pairs) or dabs (9 floats each); contour ends via [strokeContourEnds]. */
    external fun strokeSnapshot(handle: Long): FloatArray

    /** Like [strokeSnapshot] for the final geometry; the native builder is consumed. */
    external fun strokeFinish(handle: Long): FloatArray

    /** Contour ends of the last [strokeSnapshot] / [strokeFinish]; empty for stamps. */
    external fun strokeContourEnds(handle: Long): IntArray

    external fun presetName(index: Int): String

    /** Default `0xAARRGGBB` color of every preset, in `Preset::ALL` order. */
    external fun presetDefaultArgb(): IntArray

    // ---- SyncCore (`jni_sync`, `jni_sync_authoring`): see crates/penwise-ffi/src/sync_core for the blob layouts.

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

    // ---- SceneHandle (`jni_scene`, `jni_scene_tools`): every tool commits its edits to the sync core (`core`,
    // `nowMs`) in the same native call and returns the edit blob for undo bookkeeping.

    external fun sceneOpen(
        sync: Long,
        page: ByteArray,
        entropy: Long,
    ): Long

    external fun sceneFree(handle: Long)

    /** Mirrors the changes listed in a [syncReceive] outcome blob. */
    external fun sceneRefresh(
        handle: Long,
        sync: Long,
        outcome: ByteArray,
    )

    external fun sceneReload(
        handle: Long,
        sync: Long,
    )

    external fun sceneNewItemId(
        handle: Long,
        nowMs: Long,
    ): ByteArray

    /** `open != 0` begins an undo gesture, `0` ends it. */
    external fun sceneGesture(
        handle: Long,
        open: Int,
    )

    /** Bit 0 can undo, bit 1 can redo. */
    external fun sceneHistory(handle: Long): Int

    external fun sceneUndo(
        handle: Long,
        core: Long,
        nowMs: Long,
    ): ByteArray

    external fun sceneRedo(
        handle: Long,
        core: Long,
        nowMs: Long,
    ): ByteArray

    /** Live item ids on [layers] (bit set: 1 ink, 2 agent, 4 text) in paint order, 16 bytes each. */
    external fun sceneRenderOrder(
        handle: Long,
        layers: Int,
    ): ByteArray

    /** Render data of one item (layout in `render_floats.rs`); empty when not drawable. */
    external fun sceneItemFloats(
        handle: Long,
        id: ByteArray,
    ): FloatArray

    /** [brush] = `[preset, argb]`; [samples]/[times] as for [strokePush]. */
    @Suppress("LongParameterList")
    external fun sceneAddStroke(
        handle: Long,
        id: ByteArray,
        brush: IntArray,
        scale: Float,
        seed: Long,
        samples: FloatArray,
        times: IntArray,
        core: Long,
        nowMs: Long,
    ): ByteArray

    /** [geometry] = `[x0, y0, x1, y1, kindParam]`, [style] = `[kind, constrain, preset, argb, fillArgb, hasFill]`. */
    external fun sceneAddShape(
        handle: Long,
        id: ByteArray,
        geometry: FloatArray,
        style: IntArray,
        scale: Float,
        core: Long,
        nowMs: Long,
    ): ByteArray

    @Suppress("LongParameterList")
    external fun sceneResizeShape(
        handle: Long,
        id: ByteArray,
        handleCode: Int,
        x: Float,
        y: Float,
        constrain: Boolean,
        core: Long,
        nowMs: Long,
    ): ByteArray

    /** One eraser chunk; [path] is interleaved x, y (page px). */
    external fun sceneErase(
        handle: Long,
        path: FloatArray,
        radius: Float,
        precise: Boolean,
        layers: Int,
        core: Long,
        nowMs: Long,
    ): ByteArray

    /** Ids inside a lasso polygon (interleaved x, y), 16 bytes each, bottom to top. */
    external fun sceneLasso(
        handle: Long,
        polygon: FloatArray,
        layers: Int,
    ): ByteArray

    external fun sceneHitTest(
        handle: Long,
        x: Float,
        y: Float,
        tolerance: Float,
        layers: Int,
    ): ByteArray

    external fun sceneDeleteItems(
        handle: Long,
        ids: ByteArray,
        core: Long,
        nowMs: Long,
    ): ByteArray

    external fun sceneMoveItems(
        handle: Long,
        ids: ByteArray,
        dx: Float,
        dy: Float,
        core: Long,
        nowMs: Long,
    ): ByteArray

    /** Ids whose render data changed since the last call: byte 0 = 1 when everything changed, then 16-byte ids. */
    external fun sceneTakeDirty(handle: Long): ByteArray

    /** Text content of a text item, empty otherwise. */
    external fun sceneItemText(
        handle: Long,
        id: ByteArray,
    ): String

    /** [layer]: 0 ink, 1 agent, 2 text. */
    external fun sceneSetLayer(
        handle: Long,
        ids: ByteArray,
        layer: Int,
        core: Long,
        nowMs: Long,
    ): ByteArray

    /**
     * Every live frame as JSON:
     * `[{id, project, route, preset, cssW, cssH, extent, docH, boxH, rect: [x, y, w, h], patch, label, group,
     * chosen}]`, ids as 32 hex; `extent` viewport or full, `docH` measured document height (0 = unknown).
     */
    external fun sceneFrames(handle: Long): String

    /** Places a frame from `{project, route, preset, cssW?, cssH?, extent?, x, y}` on the ink layer. */
    external fun sceneAddFrame(
        handle: Long,
        json: String,
        core: Long,
        nowMs: Long,
    ): ByteArray

    /** [preset]: phone, tablet, desktop or custom; [cssW]/[cssH] only apply to custom. */
    @Suppress("LongParameterList")
    external fun sceneSetFramePreset(
        handle: Long,
        id: ByteArray,
        preset: String,
        cssW: Int,
        cssH: Int,
        core: Long,
        nowMs: Long,
    ): ByteArray

    external fun sceneSetFrameRoute(
        handle: Long,
        id: ByteArray,
        route: String,
        core: Long,
        nowMs: Long,
    ): ByteArray

    /**
     * Marks variant frame [id] as the owner's pick (or un-marks it). Choosing one un-chooses the other frames
     * of its group in the same edit, so one call is one undo step.
     */
    external fun sceneSetFrameChosen(
        handle: Long,
        id: ByteArray,
        chosen: Boolean,
        core: Long,
        nowMs: Long,
    ): ByteArray

    /**
     * Merges one `pw:layout` message for [frameId]: `{preset, scroll, doc, viewport, state,
     * rects: {key: [x, y, w, h] | null}}` (viewport CSS px; rects merge per key). True when the visible
     * output changed (repaint).
     */
    external fun sceneSetFrameLayout(
        handle: Long,
        frameId: ByteArray,
        json: String,
    ): Boolean

    /** Element keys of [frameId]'s bound strokes as a JSON array (send them with `pw:watch`). */
    external fun sceneFrameKeys(
        handle: Long,
        frameId: ByteArray,
    ): String

    /**
     * Commits a stroke drawn on a frame: [samples] are page coords (same layout as [sceneAddStroke]),
     * [bindingJson] = `{"frame":"<32hex>","mode":"bound"|"area","key":"…"?}`. Core fills the preset, state and
     * anchor from the frame's layout and groups the stroke into a mark.
     */
    @Suppress("LongParameterList")
    external fun sceneAddBoundStroke(
        handle: Long,
        id: ByteArray,
        brush: IntArray,
        scale: Float,
        seed: Long,
        samples: FloatArray,
        times: IntArray,
        bindingJson: String,
        core: Long,
        nowMs: Long,
    ): ByteArray

    /**
     * Live marks as JSON:
     * `[{id, frame, preset, status, state, strokes, rect: [x, y, w, h] | null}]`, ids as 32 hex, rect in page
     * units (`null` = hidden).
     */
    external fun sceneMarks(handle: Long): String

    /** [status]: new, seen, done or reopened (LWW, undoable, synced). */
    external fun sceneSetMarkStatus(
        handle: Long,
        markId: ByteArray,
        status: String,
        core: Long,
        nowMs: Long,
    ): ByteArray

    /** The local show-done flag; true when the visible output changed. */
    external fun sceneSetShowDone(
        handle: Long,
        on: Boolean,
    ): Boolean
}
