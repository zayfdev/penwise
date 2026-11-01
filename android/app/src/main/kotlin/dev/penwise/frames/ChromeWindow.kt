package dev.penwise.frames

import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.runtime.Composable
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
    content: @Composable () -> Unit,
) {
    val insets = rememberChromeInsets()
    val margin = with(LocalDensity.current) { MARGIN_DP.dp.roundToPx() }
    val provider =
        remember(corner, insets, margin) {
            object : PopupPositionProvider {
                override fun calculatePosition(
                    anchorBounds: IntRect,
                    windowSize: IntSize,
                    layoutDirection: LayoutDirection,
                    popupContentSize: IntSize,
                ): IntOffset {
                    val spot = ChromePlacement.corner(corner, windowSize, popupContentSize, margin, insets)
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
    content: @Composable () -> Unit,
) {
    val insets = rememberChromeInsets()
    val margin = with(LocalDensity.current) { MARGIN_DP.dp.roundToPx() }
    val provider =
        remember(x, y, insets, margin) {
            object : PopupPositionProvider {
                override fun calculatePosition(
                    anchorBounds: IntRect,
                    windowSize: IntSize,
                    layoutDirection: LayoutDirection,
                    popupContentSize: IntSize,
                ): IntOffset {
                    val spot = ChromePlacement.pinned(x, y, windowSize, popupContentSize, margin, insets)
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
