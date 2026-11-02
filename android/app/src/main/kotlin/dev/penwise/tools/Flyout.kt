package dev.penwise.tools

import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.TransformOrigin
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntRect
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Popup
import androidx.compose.ui.window.PopupPositionProvider
import androidx.compose.ui.window.PopupProperties
import dev.penwise.frames.ChromeStyle

private const val GAP_DP = 14
private const val MARGIN_DP = 12
private const val PANEL_PAD_DP = 16
private const val PANEL_GAP_DP = 16

/** A window that takes touches inside itself only, so the canvas under and around it keeps the pen and the fingers. */
private val FlyoutProperties =
    PopupProperties(
        focusable = false,
        dismissOnBackPress = false,
        dismissOnClickOutside = false,
        clippingEnabled = false,
    )

/**
 * A panel that opens above the button it is declared in, in its own window (the dock is a window too, and the ink
 * surface is below both). It grows from the button (scale and fade, [DockMotion]) and shrinks back on close; the window
 * stays alive until the close animation is done.
 */
@Composable
internal fun Flyout(
    visible: Boolean,
    content: @Composable () -> Unit,
) {
    val progress = remember { Animatable(0f) }
    var pivot by remember { mutableFloatStateOf(0.5f) }
    LaunchedEffect(visible) {
        val spec = tween<Float>(if (visible) DockMotion.OPEN_MS else DockMotion.CLOSE_MS, easing = DockMotion.easeOut)
        progress.animateTo(if (visible) 1f else 0f, spec)
    }
    if (!visible && progress.value == 0f && !progress.isRunning) return
    val density = LocalDensity.current
    val provider =
        remember(density) {
            val gap = with(density) { GAP_DP.dp.roundToPx() }
            val margin = with(density) { MARGIN_DP.dp.roundToPx() }
            object : PopupPositionProvider {
                override fun calculatePosition(
                    anchorBounds: IntRect,
                    windowSize: IntSize,
                    layoutDirection: LayoutDirection,
                    popupContentSize: IntSize,
                ): IntOffset {
                    val spot = FlyoutPlacement.above(anchorBounds, windowSize, popupContentSize, gap, margin)
                    pivot = spot.pivotX
                    return IntOffset(spot.x, spot.y)
                }
            }
        }
    Popup(popupPositionProvider = provider, properties = FlyoutProperties) {
        Column(
            Modifier.graphicsLayer {
                val grow = DockMotion.FLYOUT_FROM + (1f - DockMotion.FLYOUT_FROM) * progress.value
                alpha = progress.value
                scaleX = grow
                scaleY = grow
                transformOrigin = TransformOrigin(pivot, 1f)
            },
        ) {
            content()
        }
    }
}

/** The graphite surface of a flyout: panel fill, hairline, 18 dp corners, [width] wide, its rows [gap] apart. */
@Composable
internal fun FlyoutPanel(
    width: Dp,
    modifier: Modifier = Modifier,
    gap: Dp = PANEL_GAP_DP.dp,
    content: @Composable ColumnScope.() -> Unit,
) {
    val colors = ChromeStyle.colors
    val shape = RoundedCornerShape(ChromeStyle.PANEL_RADIUS_DP.dp)
    Column(
        modifier
            .width(width)
            .background(colors.panel, shape)
            .border(ChromeStyle.HAIRLINE_DP.dp, colors.hairline, shape)
            .padding(PANEL_PAD_DP.dp),
        verticalArrangement = Arrangement.spacedBy(gap),
        content = content,
    )
}
