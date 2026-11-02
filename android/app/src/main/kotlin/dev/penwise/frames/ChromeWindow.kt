package dev.penwise.frames

import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.runtime.Composable
import androidx.compose.runtime.compositionLocalOf
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.remember
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntRect
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Popup
import androidx.compose.ui.window.PopupPositionProvider
import androidx.compose.ui.window.PopupProperties

private const val MARGIN_DP = 12

/** Where a chrome window sits on the screen. */
internal enum class ChromeCorner { TOP_START, TOP_CENTER, TOP_END, BOTTOM_START, BOTTOM_CENTER }

/** The screen's unsafe edges (cutout, bars) in px, kept clear by the chrome windows. */
internal data class ChromeInsets(
    val left: Int = 0,
    val top: Int = 0,
    val right: Int = 0,
    val bottom: Int = 0,
)

/** A window position in px from the screen's top-left. */
internal data class ChromeSpot(
    val x: Int,
    val y: Int,
)

/** Where chrome windows go. Pure, so the placement is unit-tested without a window. */
internal object ChromePlacement {
    /** The top-left of a [popup]-sized window at [corner] of a [window] screen, [margin] clear of the [insets]. */
    fun corner(
        corner: ChromeCorner,
        window: IntSize,
        popup: IntSize,
        margin: Int,
        insets: ChromeInsets,
    ): ChromeSpot {
        val left = insets.left + margin
        val right = window.width - insets.right - margin - popup.width
        val top = insets.top + margin
        val bottom = window.height - insets.bottom - margin - popup.height
        val centre = (window.width - popup.width) / 2
        return when (corner) {
            ChromeCorner.TOP_START -> ChromeSpot(left, top)
            ChromeCorner.TOP_CENTER -> ChromeSpot(centre, top)
            ChromeCorner.TOP_END -> ChromeSpot(right, top)
            ChromeCorner.BOTTOM_START -> ChromeSpot(left, bottom)
            ChromeCorner.BOTTOM_CENTER -> ChromeSpot(centre, bottom)
        }
    }

    /**
     * [spot] moved clear of [avoid] (another chrome window's rect) when the [popup] placed there would overlap it, so
     * the frame chip and the dock never cover each other: side by side when the screen is wide enough, stacked when
     * not. Stacked means below a window in the upper half of the screen and above one in the lower half.
     */
    fun clearOf(
        spot: ChromeSpot,
        popup: IntSize,
        avoid: IntRect?,
        margin: Int,
        window: IntSize,
        insets: ChromeInsets,
    ): ChromeSpot {
        if (avoid == null) return spot
        val overlaps =
            spot.x < avoid.right + margin && spot.x + popup.width + margin > avoid.left &&
                spot.y < avoid.bottom + margin && spot.y + popup.height + margin > avoid.top
        if (!overlaps) return spot
        val minY = insets.top + margin
        val maxY = (window.height - insets.bottom - margin - popup.height).coerceAtLeast(minY)
        if (avoid.center.y > window.height / 2) {
            return ChromeSpot(spot.x, (avoid.top - margin - popup.height).coerceIn(minY, maxY))
        }
        return ChromeSpot(spot.x, (avoid.bottom + margin).coerceAtMost(maxY))
    }

    /**
     * [spot] moved sideways clear of every rect in [avoid] that the [popup] placed there would overlap: to the far side
     * of the rect on the side the popup's centre is on, then back inside the safe area. The window keeps its size, so
     * its buttons never change places; only the whole window shifts.
     */
    fun clearBeside(
        spot: ChromeSpot,
        popup: IntSize,
        avoid: List<IntRect>,
        margin: Int,
        window: IntSize,
        insets: ChromeInsets,
    ): ChromeSpot {
        var x = spot.x
        for (rect in avoid) {
            val overlaps =
                x < rect.right + margin && x + popup.width + margin > rect.left &&
                    spot.y < rect.bottom + margin && spot.y + popup.height + margin > rect.top
            if (!overlaps) continue
            val rightOf = x + popup.width / 2 >= rect.center.x
            x = if (rightOf) rect.right + margin else rect.left - margin - popup.width
        }
        val maxX = (window.width - insets.right - margin - popup.width).coerceAtLeast(insets.left + margin)
        return ChromeSpot(x.coerceIn(insets.left + margin, maxX), spot.y)
    }

    /**
     * A window that follows a point ([x], [y] px) but never leaves the safe area: the frame chip sits at its frame's
     * top-left corner and slides to the screen's edge when the frame's corner is scrolled out of view.
     */
    fun pinned(
        x: Int,
        y: Int,
        window: IntSize,
        popup: IntSize,
        margin: Int,
        insets: ChromeInsets,
    ): ChromeSpot {
        val maxX = (window.width - insets.right - margin - popup.width).coerceAtLeast(insets.left + margin)
        val maxY = (window.height - insets.bottom - margin - popup.height).coerceAtLeast(insets.top + margin)
        return ChromeSpot(x.coerceIn(insets.left + margin, maxX), y.coerceIn(insets.top + margin, maxY))
    }
}

/** A window that takes touches inside itself only: outside them the canvas under it gets the pen and the fingers. */
private val ChromeProperties =
    PopupProperties(
        focusable = false,
        dismissOnBackPress = false,
        dismissOnClickOutside = false,
        clippingEnabled = false,
    )

/**
 * Hosts [content] in its own small window above the ink surface (locked-frames contract §3.5): the surface is above the
 * activity window while frames exist, so in-window controls would sit under the ink. The window is not focusable, as
 * big as its content, and every touch outside it passes through to the canvas.
 */
@Composable
internal fun ChromeWindow(
    corner: ChromeCorner,
    report: ((IntRect) -> Unit)? = null,
    avoid: List<IntRect> = emptyList(),
    content: @Composable () -> Unit,
) {
    val insets = rememberChromeInsets()
    val margin = with(LocalDensity.current) { MARGIN_DP.dp.roundToPx() }
    val provider =
        remember(corner, insets, margin, avoid) {
            object : PopupPositionProvider {
                override fun calculatePosition(
                    anchorBounds: IntRect,
                    windowSize: IntSize,
                    layoutDirection: LayoutDirection,
                    popupContentSize: IntSize,
                ): IntOffset {
                    val placed = ChromePlacement.corner(corner, windowSize, popupContentSize, margin, insets)
                    val spot =
                        ChromePlacement.clearBeside(placed, popupContentSize, avoid, margin, windowSize, insets)
                    report?.invoke(IntRect(IntOffset(spot.x, spot.y), popupContentSize))
                    return IntOffset(spot.x, spot.y)
                }
            }
        }
    Popup(popupPositionProvider = provider, properties = ChromeProperties, content = content)
}

/** A chrome window that follows the point [x], [y] (px from the screen's top-left) within the safe area. */
@Composable
internal fun PinnedChromeWindow(
    x: Int,
    y: Int,
    avoid: List<IntRect> = emptyList(),
    content: @Composable () -> Unit,
) {
    val insets = rememberChromeInsets()
    val margin = with(LocalDensity.current) { MARGIN_DP.dp.roundToPx() }
    val provider =
        remember(x, y, avoid, insets, margin) {
            object : PopupPositionProvider {
                override fun calculatePosition(
                    anchorBounds: IntRect,
                    windowSize: IntSize,
                    layoutDirection: LayoutDirection,
                    popupContentSize: IntSize,
                ): IntOffset {
                    val pinned = ChromePlacement.pinned(x, y, windowSize, popupContentSize, margin, insets)
                    val spot =
                        avoid.fold(pinned) { at, rect ->
                            ChromePlacement.clearOf(at, popupContentSize, rect, margin, windowSize, insets)
                        }
                    return IntOffset(spot.x, spot.y)
                }
            }
        }
    Popup(popupPositionProvider = provider, properties = ChromeProperties, content = content)
}

@Composable
private fun rememberChromeInsets(): ChromeInsets {
    val density = LocalDensity.current
    val direction = LocalLayoutDirection.current
    val safe = WindowInsets.safeDrawing
    return ChromeInsets(
        safe.getLeft(density, direction),
        safe.getTop(density),
        safe.getRight(density, direction),
        safe.getBottom(density),
    )
}

/** The chrome windows that others keep clear of. */
internal enum class ChromeSlot { DOCK, SWITCHER, OVERVIEW }

/** Where the chrome windows sit, so the frame chip and the dock can keep clear of them. */
internal class ChromeAvoid {
    private val rects = mutableStateMapOf<ChromeSlot, IntRect>()

    fun report(
        slot: ChromeSlot,
        rect: IntRect,
    ) {
        if (rects[slot] != rect) rects[slot] = rect
    }

    fun clear(slot: ChromeSlot) {
        rects.remove(slot)
    }

    /** The rects of [slots] that are on screen now. */
    fun of(vararg slots: ChromeSlot): List<IntRect> = slots.mapNotNull { rects[it] }
}

private val sharedAvoid = ChromeAvoid()

internal val LocalChromeAvoid = compositionLocalOf { sharedAvoid }
