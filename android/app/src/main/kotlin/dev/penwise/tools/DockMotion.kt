package dev.penwise.tools

import androidx.compose.animation.core.CubicBezierEasing
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.interaction.InteractionSource
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.graphicsLayer

/**
 * The chrome's motion, kept small and quick so the pen is never waiting on it. Every animation here runs on the
 * Compose clock, which follows the system animator duration scale: at 0 (animations off) each one jumps to its end.
 */
internal object DockMotion {
    /** Slows down towards its end: a flyout settles into place. */
    val easeOut = CubicBezierEasing(0.2f, 0f, 0f, 1f)

    const val OPEN_MS = 180
    const val CLOSE_MS = 150
    const val SELECT_MS = 150
    const val PRESS_MS = 70
    const val FLYOUT_FROM = 0.88f
    const val PRESSED_SCALE = 0.92f
}

/** Shrinks the control a little while it is held down, the press feedback of every dock button and tile. */
@Composable
internal fun Modifier.pressScale(source: InteractionSource): Modifier {
    val pressed by source.collectIsPressedAsState()
    val scale by animateFloatAsState(
        targetValue = if (pressed) DockMotion.PRESSED_SCALE else 1f,
        animationSpec = tween(DockMotion.PRESS_MS, easing = DockMotion.easeOut),
        label = "press",
    )
    return graphicsLayer {
        scaleX = scale
        scaleY = scale
    }
}
