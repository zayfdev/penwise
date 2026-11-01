package dev.penwise.ink

/**
 * Locks the camera to [rect] (a frame's box on the board) and flies to its fit-width view in [slideMs]; the part of
 * the box on screen stays at the top when the camera already looks at it. Without a screen size yet the lock is set
 * and the first size lands on the fit.
 */
fun ViewportState.lockTo(
    rect: PageRect,
    slideMs: Long = CameraTween.DURATION_MS,
) {
    if (rect.width <= 0f || rect.height <= 0f) return
    lock = rect
    fitRect(rect, FitMode.FIT_WIDTH, slideMs)
}

/** Frees the camera (the page has no frames, or was left); it stays where it is. */
fun ViewportState.unlock() {
    lock = null
}
