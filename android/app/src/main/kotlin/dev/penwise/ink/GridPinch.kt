package dev.penwise.ink

import kotlin.math.max

/**
 * The dead zone of the overview pinch (locked-frames contract §3.1). A two-finger gesture that starts at the
 * fit scale accumulates its pinch factor as a ratio of the start span. While the ratio is under 1 the camera
 * stays at fit and only shows a rubber band ([RubberBand]); above 1 it zooms in as [effective] says. At
 * [RELEASE_RATIO] or less the overview opens when the fingers lift, at [IMMEDIATE_RATIO] or less at once; a
 * pinch that ends short of that springs back. A gesture that did not start at fit is never an overview pinch,
 * and one that is cancelled (pen, palm, a third finger) never opens anything. Pure logic. UI thread only.
 */
internal class GridPinch {
    /** What a finished gesture asks for. */
    enum class Outcome { NONE, OPEN_GRID, SPRING_BACK }

    /** The gesture started at the fit scale and is still being followed. */
    var armed = false
        private set

    private var opened = false
    private var maxRatio = Float.MAX_VALUE

    /** The pinch so far as a ratio of the start span (1 = none), kept within the zoom range. */
    var ratio = 1f
        private set

    /** The zoom the camera should have relative to the start: 1 while pinching out, [ratio] when spreading. */
    val effective: Float get() = if (armed) max(1f, ratio) else 1f

    /** A two-finger gesture starts; [atFitScale]: the camera is at minimum scale now; [zoomRatio]: max / min. */
    fun begin(
        atFitScale: Boolean,
        zoomRatio: Float = Float.MAX_VALUE,
    ) {
        armed = atFitScale
        opened = false
        ratio = 1f
        maxRatio = zoomRatio
    }

    /** One pinch step of [factor] (span now / span before); true when the overview must open right now. */
    fun step(factor: Float): Boolean {
        if (!armed || opened) return false
        ratio = (ratio * factor).coerceAtMost(maxRatio)
        opened = ratio <= IMMEDIATE_RATIO
        return opened
    }

    /** The gesture was cancelled: nothing opens, nothing is left armed. */
    fun cancel() {
        armed = false
        opened = false
        ratio = 1f
    }

    /** The fingers lifted. */
    fun end(): Outcome {
        val outcome =
            when {
                !armed || opened -> Outcome.NONE
                ratio <= RELEASE_RATIO -> Outcome.OPEN_GRID
                ratio < 1f -> Outcome.SPRING_BACK
                else -> Outcome.NONE
            }
        cancel()
        return outcome
    }

    companion object {
        /** Pinched out to this much of the start span or less: the overview opens on release. */
        const val RELEASE_RATIO = 0.82f

        /** Pinched out to this much or less: the overview opens at once, fingers still down. */
        const val IMMEDIATE_RATIO = 0.70f
    }
}

/** The camera's give below the fit scale while an overview pinch runs: it shrinks a little and resists. */
internal object RubberBand {
    /** The camera never shows less than this share of the fit scale. */
    const val FLOOR = 0.7f

    /** Share of the pinch that shows. */
    const val RESISTANCE = 0.5f

    /** The scale factor (at most 1) to show for a pinch [ratio]: 1 at or above 1, down to [FLOOR]. */
    fun dip(ratio: Float): Float = if (ratio >= 1f) 1f else max(FLOOR, 1f - RESISTANCE * (1f - ratio))

    /** [view] scaled by [factor] about the screen point ([cx], [cy]). */
    fun about(
        view: ViewTransform,
        factor: Float,
        cx: Float,
        cy: Float,
    ): ViewTransform = ViewTransform(view.scale * factor, cx - (cx - view.tx) * factor, cy - (cy - view.ty) * factor)
}
