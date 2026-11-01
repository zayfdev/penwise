package dev.penwise.ink

/**
 * The maths of a locked camera (locked-frames contract §3.1): the screen stays inside one frame's box. Horizontally
 * the box is exactly as wide as the screen at the fit-width scale, which is also the smallest scale allowed;
 * vertically the screen stays between the box's top and bottom. A box shorter than the screen is centred.
 */
internal object LockClamp {
    /** The fit-width scale of [lock] on a [viewWidth] screen: the smallest zoom while locked (at most [maxScale]). */
    fun minScale(
        lock: PageRect,
        viewWidth: Float,
        maxScale: Float,
    ): Float = (viewWidth / lock.width).coerceAtMost(maxScale)

    /** [view] moved to the nearest transform whose screen lies inside [lock] at a scale of at least [minScale]. */
    fun clamp(
        view: ViewTransform,
        lock: PageRect,
        viewWidth: Float,
        viewHeight: Float,
        maxScale: Float,
    ): ViewTransform {
        val scale = view.scale.coerceIn(minScale(lock, viewWidth, maxScale), maxScale)
        return ViewTransform(
            scale,
            axis(view.tx, lock.x * scale, lock.right * scale, viewWidth),
            axis(view.ty, lock.y * scale, lock.bottom * scale, viewHeight),
        )
    }

    /** The fit-width view of [lock] from its top: the camera a lock starts at. */
    fun fitTop(
        lock: PageRect,
        viewWidth: Float,
        viewHeight: Float,
        maxScale: Float,
    ): ViewTransform = fitAt(lock, lock.y, viewWidth, viewHeight, maxScale)

    /** The fit-width view of [lock] with page y [top] at the screen top (a refit after the screen or box changed). */
    fun fitAt(
        lock: PageRect,
        top: Float,
        viewWidth: Float,
        viewHeight: Float,
        maxScale: Float,
    ): ViewTransform {
        val scale = minScale(lock, viewWidth, maxScale)
        val tx = (viewWidth - lock.width * scale) / 2f - lock.x * scale
        return clamp(ViewTransform(scale, tx, -top * scale), lock, viewWidth, viewHeight, maxScale)
    }

    /** The offset that keeps the screen `0..view` inside `low..high` (box edges in screen px at offset 0). */
    private fun axis(
        offset: Float,
        low: Float,
        high: Float,
        view: Float,
    ): Float {
        val most = -low
        val least = view - high
        return if (least > most) (least + most) / 2f else offset.coerceIn(least, most)
    }
}
