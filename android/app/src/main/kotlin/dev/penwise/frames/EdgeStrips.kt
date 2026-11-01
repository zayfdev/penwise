package dev.penwise.frames

import kotlin.math.abs

/**
 * The screen-edge strips the app keeps from the system gestures (Android caps an exclusion at 200 dp of height per
 * edge, so they sit vertically centred on each edge). The edge swipe only starts in them, so the system's Back gesture
 * can never take the touch: [WIDTH_DP] is the exclusion's width, the swipe uses its outer [EDGE_DP]. One definition
 * for the exclusion in `CanvasScreen` and for the classifier.
 */
object EdgeStrips {
    const val WIDTH_DP = 48
    const val HEIGHT_DP = 200

    /** An edge swipe starts within this of the screen edge. */
    const val EDGE_DP = 24

    /** Is ([x], [y]) in a strip of a [viewWidth] by [viewHeight] screen: [edgePx] wide, [heightPx] tall? */
    fun contains(
        x: Float,
        y: Float,
        viewWidth: Float,
        viewHeight: Float,
        edgePx: Float,
        heightPx: Float,
    ): Boolean = abs(y - viewHeight / 2f) <= heightPx / 2f && (x <= edgePx || x >= viewWidth - edgePx)
}
