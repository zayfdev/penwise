package dev.penwise.tools

import androidx.compose.ui.unit.IntRect
import androidx.compose.ui.unit.IntSize

/** Where a flyout sits: its top-left in px, and the 0 to 1 horizontal point of its bottom edge the button is under. */
internal data class FlyoutSpot(
    val x: Int,
    val y: Int,
    val pivotX: Float,
)

/** Where a dock flyout goes. Pure, so it is unit-tested without a window. */
internal object FlyoutPlacement {
    /**
     * The spot of a [flyout]-sized panel above the [anchor] button, [gap] clear of it: centred on the button, then slid
     * back inside the [window] by [margin]. The panel opens from the point of its bottom edge that is over the button.
     */
    fun above(
        anchor: IntRect,
        window: IntSize,
        flyout: IntSize,
        gap: Int,
        margin: Int,
    ): FlyoutSpot {
        val maxX = (window.width - margin - flyout.width).coerceAtLeast(margin)
        val x = (anchor.center.x - flyout.width / 2).coerceIn(margin, maxX)
        val y = (anchor.top - gap - flyout.height).coerceAtLeast(margin)
        val pivot = ((anchor.center.x - x).toFloat() / flyout.width.coerceAtLeast(1)).coerceIn(0f, 1f)
        return FlyoutSpot(x, y, pivot)
    }
}
