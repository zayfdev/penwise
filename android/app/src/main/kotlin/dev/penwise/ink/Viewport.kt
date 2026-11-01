package dev.penwise.ink

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlin.math.min

/** Page → screen mapping: `screen = page * scale + (tx, ty)`. A plain value; the render thread reads snapshots. */
data class ViewTransform(
    val scale: Float,
    val tx: Float,
    val ty: Float,
) {
    fun toScreenX(pageX: Float): Float = pageX * scale + tx

    fun toScreenY(pageY: Float): Float = pageY * scale + ty

    fun toPageX(screenX: Float): Float = (screenX - tx) / scale

    fun toPageY(screenY: Float): Float = (screenY - ty) / scale

    companion object {
        val IDENTITY = ViewTransform(1f, 0f, 0f)
    }
}

/**
 * Where the page sits on the screen. Pan and zoom are owned here and driven by two-finger gestures; the stylus, the
 * renderer and the gestures all read [transform]. Zoom stays in [MIN_SCALE]..[MAX_SCALE] (or down to the fit
 * scale when the screen is so small that the whole page needs less; an infinite page zooms out to
 * [MIN_SCALE_INFINITE], so a full-length frame fits whole). A fixed page cannot be flung off screen: a margin of it
 * always stays visible. An infinite page pans freely. [fitRect] flies the camera to a rect. Mutated on the UI thread
 * only.
 */
class ViewportState {
    @Volatile
    var transform: ViewTransform = ViewTransform.IDENTITY
        private set

    /** True while a two-finger gesture runs: the renderer may then show a scaled copy of the last render. */
    @Volatile
    var gesturing: Boolean = false
        private set

    private val _zoom = MutableStateFlow(1f)

    /** The current scale, `1f` = 100 % (one page px per screen px). */
    val zoom: StateFlow<Float> = _zoom.asStateFlow()

    /**
     * Called after every change of [transform] or [gesturing], with the new transform: the canvas repaints from it, the
     * paper and the frames move with it.
     */
    internal val observers = ViewObservers()

    /** Asks for animation frames while a fit flies; installed by the canvas view. */
    internal var driver: CameraDriver? = null

    /** The fit flight in progress; the canvas view's driver steps it, a touch cancels it. */
    internal val flight = CameraFlight { apply(it) }

    private var viewWidth = 0f
    private var viewHeight = 0f
    private var page = PageStyle.BLANK
    private var fitted = true

    /** The screen the page is shown on (view size in px). Refits when the view was at the fitted position. */
    internal fun setViewSize(
        width: Float,
        height: Float,
    ) {
        if (width <= 0f || height <= 0f || (width == viewWidth && height == viewHeight)) return
        viewWidth = width
        viewHeight = height
        if (fitted) apply(fit()) else apply(clamped(transform))
    }

    /** Switches to another sheet; a different sheet starts fitted. */
    internal fun setPage(style: PageStyle) {
        if (style.infinite == page.infinite && style.widthPx == page.widthPx && style.heightPx == page.heightPx) return
        page = style
        reset()
    }

    /** The canvas has told the camera its size, so a [fitRect] can land. */
    val hasSize: Boolean get() = viewWidth > 0f && viewHeight > 0f

    /** True while a [fitRect] flight runs. */
    val animating: Boolean get() = flight.active

    /**
     * Flies the camera to show [rect] by [mode] (220 ms, ease-out-cubic). Without a [driver] the camera jumps. Any
     * touch cancels the flight where it is (`flight.cancel()`).
     */
    fun fitRect(
        rect: PageRect,
        mode: FitMode,
    ) {
        if (viewWidth <= 0f || viewHeight <= 0f) return
        val screen = CameraScreen(viewWidth, viewHeight, minScale, MAX_SCALE)
        val target = CameraFit.target(rect, mode, screen, transform)
        flight.cancel()
        fitted = false
        val driver = driver
        if (driver == null || target == transform) {
            apply(target)
            return
        }
        flight.start(CameraTween(transform, target, viewWidth, viewHeight))
        driver.request()
    }

    /** Fits the page to the screen (an infinite page: 1:1 from its origin). */
    fun reset() {
        flight.cancel()
        fitted = true
        setGesturing(false)
        apply(fit())
    }

    /**
     * One step of a two-finger gesture: the content under `(fromX, fromY)` moves to `(toX, toY)` while the scale
     * changes by [factor]. The page point under the starting focal point ends up under the new one.
     */
    internal fun transformBy(
        factor: Float,
        fromX: Float,
        fromY: Float,
        toX: Float,
        toY: Float,
    ) {
        val current = transform
        val scale = (current.scale * factor).coerceIn(minScale, MAX_SCALE)
        val pageX = current.toPageX(fromX)
        val pageY = current.toPageY(fromY)
        fitted = false
        apply(clamped(ViewTransform(scale, toX - pageX * scale, toY - pageY * scale)))
    }

    /** Marks a two-finger gesture as running or settled; settling repaints at full quality. */
    internal fun setGesturing(active: Boolean) {
        if (gesturing == active) return
        gesturing = active
        observers.notify(transform)
    }

    private val minScale: Float get() = if (page.infinite) MIN_SCALE_INFINITE else min(MIN_SCALE, fitScale)

    private val fitScale: Float
        get() =
            if (page.infinite || viewWidth <= 0f || page.widthPx <= 0f || page.heightPx <= 0f) {
                1f
            } else {
                min(viewWidth / page.widthPx, viewHeight / page.heightPx)
            }

    private fun fit(): ViewTransform {
        if (page.infinite || viewWidth <= 0f) return ViewTransform.IDENTITY
        val scale = fitScale.coerceIn(minScale, MAX_SCALE)
        return ViewTransform(
            scale,
            (viewWidth - page.widthPx * scale) / 2f,
            (viewHeight - page.heightPx * scale) / 2f,
        )
    }

    private fun clamped(view: ViewTransform): ViewTransform {
        if (page.infinite || viewWidth <= 0f) return view
        return ViewTransform(
            view.scale,
            clampAxis(view.tx, page.widthPx * view.scale, viewWidth),
            clampAxis(view.ty, page.heightPx * view.scale, viewHeight),
        )
    }

    /** Any page point may sit anywhere on screen, as long as [KEEP_PX] of the page (or all of a smaller page) shows. */
    private fun clampAxis(
        offset: Float,
        pageSize: Float,
        viewSize: Float,
    ): Float {
        val keep = min(KEEP_PX, min(pageSize, viewSize))
        return offset.coerceIn(keep - pageSize, viewSize - keep)
    }

    private fun apply(next: ViewTransform) {
        if (next == transform) return
        transform = next
        _zoom.value = next.scale
        observers.notify(next)
    }

    companion object {
        const val MIN_SCALE = 0.5f

        /** An infinite page zooms out this far: a 10 000 px frame still fits a 1600 px screen. */
        const val MIN_SCALE_INFINITE = 0.05f
        const val MAX_SCALE = 8f

        /** Screen px of a fixed page that always stay visible. */
        const val KEEP_PX = 64f
    }
}

/** Whoever supplies animation frames to a [ViewportState] flight (the canvas view's Choreographer). */
internal fun interface CameraDriver {
    /** A flight began: call `stepAnimation` on every frame until it returns false. */
    fun request()
}

/**
 * Who follows the canvas view: the ink surface, the paper view, the frames. UI thread only. [notify] runs on every
 * camera move, so it walks an array that is replaced (never edited) when an observer comes or goes: an observer that
 * detaches while it runs is safe, and a move allocates nothing.
 */
internal class ViewObservers {
    private var observers: Array<(ViewTransform) -> Unit> = emptyArray()

    fun add(observer: (ViewTransform) -> Unit) {
        if (observers.contains(observer)) return
        observers += observer
    }

    fun remove(observer: (ViewTransform) -> Unit) {
        observers = observers.filter { it != observer }.toTypedArray()
    }

    /** Tells every observer the view is now [transform]. */
    fun notify(transform: ViewTransform) {
        val snapshot = observers
        for (index in snapshot.indices) snapshot[index](transform)
    }
}

/** Pans the camera by ([dx], [dy]) screen px: the scroll link carries the board along with a page's own scroll. */
internal fun ViewportState.panBy(
    dx: Float,
    dy: Float,
) = transformBy(1f, 0f, 0f, dx, dy)
