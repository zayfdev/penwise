package dev.penwise.core

/**
 * JNI surface of the page scene (`jni_scene`, `jni_scene_tools`): every tool commits its edits to the sync core
 * (`core`, `nowMs`) in the same native call and returns the edit blob for undo bookkeeping. Wrapped by `PageScene`.
 */
@Suppress("TooManyFunctions") // one object per JNI class: the symbol names pin it
internal object NativeScene {
    init {
        System.loadLibrary("penwise_ffi")
    }

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
}
