package dev.penwise.ink

import kotlin.math.exp
import kotlin.math.ln
import kotlin.math.max
import kotlin.math.min
import kotlin.math.pow

/** How a rect is fitted to the screen. */
enum class FitMode {
    /** The focus view: the rect's width fills the screen width and the top of the visible part is kept. */
    FIT_WIDTH,

    /** The board view: the whole rect is visible, centred. */
    FIT_ALL,
}

/** A rect in page units. */
data class PageRect(
    val x: Float,
    val y: Float,
    val width: Float,
    val height: Float,
) {
    val right: Float get() = x + width
    val bottom: Float get() = y + height
}

/** The screen the camera fits into, and the zoom range it may use. */
internal data class CameraScreen(
    val width: Float,
    val height: Float,
    val minScale: Float,
    val maxScale: Float,
)

/** Where the camera ends up for a fit; pure maths, so the focus view is tested without a screen. */
internal object CameraFit {
    /** The transform that shows [rect] on [screen] by [mode], starting from the camera [current]. */
    fun target(
        rect: PageRect,
        mode: FitMode,
        screen: CameraScreen,
        current: ViewTransform,
    ): ViewTransform {
        if (rect.width <= 0f || rect.height <= 0f || screen.width <= 0f || screen.height <= 0f) return current
        return when (mode) {
            FitMode.FIT_WIDTH -> fitWidth(rect, screen, current)
            FitMode.FIT_ALL -> fitAll(rect, screen)
        }
    }

    private fun fitWidth(
        rect: PageRect,
        screen: CameraScreen,
        current: ViewTransform,
    ): ViewTransform {
        val scale = (screen.width / rect.width).coerceIn(screen.minScale, screen.maxScale)
        // The part of the rect on screen now stays at the top; a rect off screen starts at its own top.
        val topNow = current.toPageY(0f)
        val bottomNow = current.toPageY(screen.height)
        val seen = bottomNow > rect.y && topNow < rect.bottom
        val lowest = max(rect.y, rect.bottom - screen.height / scale)
        val top = if (seen) topNow.coerceIn(rect.y, lowest) else rect.y
        val tx = (screen.width - rect.width * scale) / 2f - rect.x * scale
        return ViewTransform(scale, tx, -top * scale)
    }

    private fun fitAll(
        rect: PageRect,
        screen: CameraScreen,
    ): ViewTransform {
        val scale =
            min(
                screen.width / rect.width,
                screen.height / rect.height,
            ).coerceIn(screen.minScale, screen.maxScale)
        val tx = (screen.width - rect.width * scale) / 2f - rect.x * scale
        val ty = (screen.height - rect.height * scale) / 2f - rect.y * scale
        return ViewTransform(scale, tx, ty)
    }
}

/**
 * The camera's flight between two transforms: the page point at the screen centre moves in a line while the scale
 * changes geometrically, so a 35x zoom reads as one steady move. Eased with ease-out-cubic over [DURATION_MS].
 */
internal class CameraTween(
    private val from: ViewTransform,
    val to: ViewTransform,
    private val viewWidth: Float,
    private val viewHeight: Float,
) {
    private val fromX = from.toPageX(viewWidth / 2f)
    private val fromY = from.toPageY(viewHeight / 2f)
    private val toX = to.toPageX(viewWidth / 2f)
    private val toY = to.toPageY(viewHeight / 2f)

    /** The transform [elapsedMs] after the start; [to] exactly once the time is up. */
    fun at(elapsedMs: Long): ViewTransform {
        if (elapsedMs >= DURATION_MS) return to
        val eased = easeOutCubic((elapsedMs.coerceAtLeast(0L)).toFloat() / DURATION_MS)
        val scale = exp(ln(from.scale) + (ln(to.scale) - ln(from.scale)) * eased)
        val cx = fromX + (toX - fromX) * eased
        val cy = fromY + (toY - fromY) * eased
        return ViewTransform(scale, viewWidth / 2f - cx * scale, viewHeight / 2f - cy * scale)
    }

    companion object {
        const val DURATION_MS = 220L

        fun easeOutCubic(t: Float): Float = 1f - (1f - t.coerceIn(0f, 1f)).pow(CUBIC)

        private const val CUBIC = 3
    }
}

/** The flight in progress, if any: stepped by the frame clock, cancelled by a touch. UI thread only. */
internal class CameraFlight(
    private val onFrame: (ViewTransform) -> Unit,
) {
    private var tween: CameraTween? = null
    private var startMs = NOT_STARTED

    /** True while a flight runs. */
    val active: Boolean get() = tween != null

    fun start(next: CameraTween) {
        tween = next
        startMs = NOT_STARTED
    }

    /** One animation frame at [frameTimeMs]; true while the flight goes on. The last frame is exactly the target. */
    fun step(frameTimeMs: Long): Boolean {
        val current = tween ?: return false
        if (startMs == NOT_STARTED) startMs = frameTimeMs
        val elapsed = frameTimeMs - startMs
        if (elapsed >= CameraTween.DURATION_MS) tween = null
        onFrame(current.at(elapsed))
        return tween != null
    }

    /** Stops where it is. */
    fun cancel() {
        tween = null
    }

    private companion object {
        const val NOT_STARTED = Long.MIN_VALUE
    }
}
