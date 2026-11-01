package dev.penwise.ink

private const val SPRING_MS = 160L

/** The locked box changed size (not place): the camera stays and is pulled inside the new box. */
fun ViewportState.relock(rect: PageRect) {
    if (lock == null || rect.width <= 0f || rect.height <= 0f) return
    val wasAtFit = atFitScale
    val top = transform.toPageY(0f)
    lock = rect
    if (viewWidth <= 0f || flight.active) return
    // A box that got another width (a preset change) is refitted from the same page y when it was fitted.
    val max = ViewportState.MAX_SCALE
    apply(if (wasAtFit) LockClamp.fitAt(rect, top, viewWidth, viewHeight, max) else clamped(transform))
}

/**
 * The overview pinch's rubber band: the camera shows [amount] (at most 1) of the scale it really has, about the
 * screen centre. The next `transformBy` starts from the real camera again.
 */
internal fun ViewportState.dipTo(amount: Float) {
    val base = undipped()
    dip = amount
    apply(RubberBand.about(base, amount, viewWidth / 2f, viewHeight / 2f))
}

/** The overview pinch is over: the camera springs from its dip back to the real camera in [SPRING_MS]. */
internal fun ViewportState.releaseDip() {
    if (dip == 1f) return
    val base = undipped()
    dip = 1f
    val driver = driver
    if (driver == null || base == transform) {
        apply(base)
        return
    }
    flight.start(CameraTween(transform, base, viewWidth, viewHeight, SPRING_MS))
    driver.request()
}

/** The real camera under the rubber band's dip. */
internal fun ViewportState.undipped(): ViewTransform =
    if (dip == 1f) transform else RubberBand.about(transform, 1f / dip, viewWidth / 2f, viewHeight / 2f)
