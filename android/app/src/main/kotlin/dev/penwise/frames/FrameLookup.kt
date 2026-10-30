package dev.penwise.frames

import dev.penwise.ink.ViewTransform

/** Where the frames are: their boxes on the screen under the camera, and which one a window point falls on. */
internal class FrameLookup(
    private val specs: () -> List<FrameSpec>,
    private val transform: () -> ViewTransform,
    private val live: () -> FrameEntry?,
) {
    /** The frames now, in paint order. */
    fun specs(): List<FrameSpec> = specs.invoke()

    fun spec(id: String): FrameSpec? = specs().firstOrNull { it.id == id }

    /** The box of [spec] on the screen under the camera [view]. */
    fun boxOnScreen(
        spec: FrameSpec,
        view: ViewTransform = transform(),
    ): ScreenBox {
        val left = view.toScreenX(spec.x)
        val top = view.toScreenY(spec.y)
        return ScreenBox(spec.id, left, top, left + spec.cssWidth * view.scale, top + spec.boxHeight * view.scale)
    }

    /** The live page when the window point is on its window (the part that is not a still). */
    fun liveAt(
        x: Float,
        y: Float,
    ): FrameEntry? = live()?.takeIf { it.isShown && it.placement.contains(x, y) }

    /** The topmost frame whose box holds the window point. */
    fun specAt(
        x: Float,
        y: Float,
    ): FrameSpec? {
        val view = transform()
        val all = specs()
        for (index in all.indices.reversed()) {
            val spec = all[index]
            val left = view.toScreenX(spec.x)
            val top = view.toScreenY(spec.y)
            if (x >= left && x < left + spec.cssWidth * view.scale && y >= top &&
                y < top + spec.boxHeight * view.scale
            ) {
                return spec
            }
        }
        return null
    }

    fun boardX(windowX: Float): Float = transform().toPageX(windowX)

    fun boardY(windowY: Float): Float = transform().toPageY(windowY)

    /** The frames whose box meets a [width] × [height] screen. */
    fun visibleIds(
        width: Int,
        height: Int,
    ): List<String> {
        val view = transform()
        return specs()
            .filter { spec ->
                val box = boxOnScreen(spec, view)
                box.right > 0f && box.left < width && box.bottom > 0f && box.top < height
            }.map { it.id }
    }
}
