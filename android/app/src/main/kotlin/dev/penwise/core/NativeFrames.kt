package dev.penwise.core

/**
 * JNI surface of the design harness' frames and marks (`jni_frames`): the frame list, presets, routes, layouts,
 * bound strokes, mark status and frame space. Wrapped by `PageScene`.
 */
@Suppress("TooManyFunctions") // one object per JNI class: the symbol names pin it
internal object NativeFrames {
    init {
        System.loadLibrary("penwise_ffi")
    }

    /**
     * Every live frame as JSON:
     * `[{id, hex, project, route, preset, cssW, cssH, extent, docH, boxH, rect: [x, y, w, h], patch, label, group,
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

    /**
     * Drops [frameId]'s layout entirely, so the next [sceneSetFrameLayout] replaces rather than merges (a live
     * frame demoted to its still). True when a layout existed.
     */
    external fun sceneClearFrameLayout(
        handle: Long,
        frameId: ByteArray,
    ): Boolean

    /**
     * Replaces [frameId]'s layout with [json] in one step, under one lock: nothing of the old layout survives and no
     * state in between is visible (a live frame demoted to its still). True when the output may have changed.
     */
    external fun sceneReplaceFrameLayout(
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

    /** Frame [frameId]'s live view on the board, `[x, y, w, h]`; empty for an unknown frame. */
    external fun frameLiveWindow(
        handle: Long,
        frameId: ByteArray,
    ): FloatArray

    /** The document point of frame [frameId] under board point ([x], [y]), `[x, y]`; empty when unknown. */
    external fun frameDocPoint(
        handle: Long,
        frameId: ByteArray,
        x: Float,
        y: Float,
    ): FloatArray

    /** The board point of document point ([x], [y]) of frame [frameId] (inverse of [frameDocPoint]). */
    external fun frameBoardPoint(
        handle: Long,
        frameId: ByteArray,
        x: Float,
        y: Float,
    ): FloatArray
}
