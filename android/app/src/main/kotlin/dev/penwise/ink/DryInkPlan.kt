package dev.penwise.ink

/** What the in-window ink last recorded: the view it was drawn under and the page area it covers. */
internal class RecordedInk(
    val view: ViewTransform,
    val band: Bounds,
)

/** How content recorded under one view is drawn under another: `screen' = screen * scale + (dx, dy)`. */
internal class InkShift(
    val scale: Float,
    val dx: Float,
    val dy: Float,
)

/**
 * The maths of the in-window dried ink ([DryInkView]), apart from any view so it is tested without a device. The ink
 * is recorded once as a display list under one camera view, for the screen plus one screen on every side, and every
 * frame draws that list through the camera's current transform. A scroll or a pan therefore only changes a matrix in
 * the same window frame as the page, which is what keeps ink and page in step.
 */
internal object DryInkPlan {
    /** The page area recorded around [view] on a [width] x [height] screen: the screen and one screen each way. */
    fun band(
        view: ViewTransform,
        width: Int,
        height: Int,
    ): Bounds {
        val w = width / view.scale
        val h = height / view.scale
        val left = view.toPageX(0f)
        val top = view.toPageY(0f)
        return Bounds(left - w, top - h, left + 2f * w, top + 2f * h)
    }

    /**
     * Must the ink be recorded again for [view]? Yes when nothing is recorded, the scene changed, or the screen left
     * the recorded band; and when the zoom changed and no gesture or flight is [moving] (until then the list is drawn
     * scaled, and recorded again once the camera rests).
     */
    fun needsRecord(
        recorded: RecordedInk?,
        view: ViewTransform,
        width: Int,
        height: Int,
        sceneChanged: Boolean,
        moving: Boolean,
    ): Boolean {
        if (recorded == null || sceneChanged) return true
        if (!covers(recorded.band, view, width, height)) return true
        return recorded.view.scale != view.scale && !moving
    }

    /** Draws content recorded under [from] as it looks under [to]. */
    fun shift(
        from: ViewTransform,
        to: ViewTransform,
    ): InkShift {
        val k = to.scale / from.scale
        return InkShift(k, to.tx - k * from.tx, to.ty - k * from.ty)
    }

    private fun covers(
        band: Bounds,
        view: ViewTransform,
        width: Int,
        height: Int,
    ): Boolean =
        view.toPageX(0f) >= band.left &&
            view.toPageY(0f) >= band.top &&
            view.toPageX(width.toFloat()) <= band.right &&
            view.toPageY(height.toFloat()) <= band.bottom
}
