package dev.penwise.overview

import dev.penwise.ink.PageRect
import kotlin.math.roundToInt

/** One transform gesture step: [zoom] about ([centreX], [centreY]) and a pan of ([panX], [panY]), in screen px. */
internal data class ViewerGesture(
    val zoom: Float,
    val centreX: Float,
    val centreY: Float,
    val panX: Float,
    val panY: Float,
)

/**
 * Where the viewer's picture sits on the screen: shown at [scale] (1 = the picture fits the screen's width) with its
 * top-left at ([x], [y]) px. Fingers move it (one finger pans, two zoom and pan); the pen does nothing, since the
 * viewer never edits. Pure, so the clamp is tested.
 */
internal data class ViewerTransform(
    val scale: Float = 1f,
    val x: Float = 0f,
    val y: Float = 0f,
) {
    /**
     * This transform after [gesture], for the picture and screen of [size]. The picture never leaves the screen's
     * width, never shows past its top or bottom, and never shrinks below fitting the width.
     */
    fun gesture(
        gesture: ViewerGesture,
        size: ViewerSize,
    ): ViewerTransform {
        val next = (scale * gesture.zoom).coerceIn(1f, MAX_SCALE)
        val grown = next / scale
        val left = minOf(0f, size.screenWidth - size.pictureWidth * next)
        val top = minOf(0f, size.screenHeight - size.pictureHeight * next)
        return ViewerTransform(
            scale = next,
            x = (gesture.centreX - (gesture.centreX - x) * grown + gesture.panX).coerceIn(left, 0f),
            y = (gesture.centreY - (gesture.centreY - y) * grown + gesture.panY).coerceIn(top, 0f),
        )
    }

    /** The picture row (px at scale 1) at the screen's top edge: 0 when the picture's top is in view. */
    fun topRow(): Float = -y / scale

    companion object {
        const val MAX_SCALE = 6f
    }
}

/** The picture's laid-out box in px. The picture is placed at the screen's top-left, never centred. */
internal data class ViewerBox(
    val left: Int,
    val top: Int,
    val width: Int,
    val height: Int,
)

/** The viewer's picture and screen, in px: the picture is laid out fitting the screen's width. */
internal data class ViewerSize(
    val pictureWidth: Float,
    val pictureHeight: Float,
    val screenWidth: Float,
    val screenHeight: Float,
) {
    /** The box the picture is laid out in at scale 1: the screen's top-left corner, as wide as the screen. */
    fun box(): ViewerBox = ViewerBox(0, 0, pictureWidth.roundToInt(), pictureHeight.roundToInt())

    companion object {
        /** A [width] by [height] px picture fitted to a screen [screenWidth] by [screenHeight] px wide. */
        fun fitted(
            width: Int,
            height: Int,
            screenWidth: Float,
            screenHeight: Float,
        ): ViewerSize {
            val ratio = if (width > 0) height.toFloat() / width else 1f
            return ViewerSize(screenWidth, screenWidth * ratio, screenWidth, screenHeight)
        }
    }
}

/** Where a new screen (frame) goes on the board: under the lowest frame, at the left edge of the lanes. */
internal object NewScreenPlacement {
    private const val GAP = 160f

    fun at(boxes: List<PageRect>): Pair<Float, Float> {
        if (boxes.isEmpty()) return 0f to 0f
        return boxes.minOf { it.x } to boxes.maxOf { it.y + it.height } + GAP
    }
}
