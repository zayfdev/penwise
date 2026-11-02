package dev.penwise.core

/**
 * JNI surface of `crates/penwise-ffi` (`libpenwise_ffi.so`) for the live stroke and the presets. The sync core is
 * [NativeSync], the page scene [NativeScene], the design-harness frames and marks [NativeFrames]; JNI symbol
 * names follow the class, so a function lives in the object `scripts/check-jni-symbols.sh` finds it in.
 * Wrapped by [InkStroke] and [InkPresets]; nothing else should call it. Native errors and panics surface as
 * `RuntimeException`.
 */
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

    /** What an `0xAARRGGBB` ink means to the agent: `problem`, `idea`, `good` or `none` (core `marks::colour`). */
    external fun colourMeaning(argb: Int): String
}
