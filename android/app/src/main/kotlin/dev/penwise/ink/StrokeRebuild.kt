package dev.penwise.ink

import android.util.Log

/** Called with a stroke the engine refused and the reason. */
internal typealias SkipReport = (SavedStroke, Throwable) -> Unit

/**
 * Geometry rebuilds that cannot take the app down: the engine rejects strokes it cannot build (an out-of-range
 * preset or scale arriving from the server or a damaged file), and one such stroke must cost only itself.
 */
internal object StrokeRebuild {
    private const val TAG = "PenwiseInk"

    val logSkip: SkipReport = { stroke, error ->
        Log.w(TAG, "skipping unbuildable stroke seed=${stroke.seed} preset=${stroke.presetIndex}: ${error.message}")
    }

    /**
     * True when `stroke` has geometry afterwards; a failure is reported through [onSkip] and returns false.
     * Boundary guard: any failure building one stroke (bad preset, bad samples) skips it, never crashes the load.
     */
    @Suppress("TooGenericExceptionCaught")
    fun tryBuild(
        stroke: SavedStroke,
        build: (SavedStroke) -> Unit = { it.rebuild() },
        onSkip: SkipReport = logSkip,
    ): Boolean {
        if (stroke.geometry != null) return true
        return try {
            build(stroke)
            true
        } catch (error: RuntimeException) {
            onSkip(stroke, error)
            false
        }
    }

    /** The strokes of `strokes` that could be built, in order. */
    fun usable(
        strokes: List<SavedStroke>,
        build: (SavedStroke) -> Unit = { it.rebuild() },
        onSkip: SkipReport = logSkip,
    ): List<SavedStroke> = strokes.filter { tryBuild(it, build, onSkip) }
}
