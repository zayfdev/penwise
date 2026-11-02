package dev.penwise.ink

import android.graphics.BlendMode
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.RectF
import androidx.core.graphics.withClip
import androidx.graphics.lowlatency.CanvasFrontBufferedRenderer
import androidx.graphics.surface.SurfaceControlCompat

/**
 * Front layer: the newest frame of the gesture in flight, drawn through the [viewport] transform (clearing only the
 * screen area the previous frame covered). Multi-buffered layer: one `drawBitmap` of the [DryLayer], which holds the
 * paper and the committed items, repaints only what the scene reported dirty and re-renders off-thread after the view
 * changed. [paper] turns the paper off while live pages sit under the surface (the surface is then translucent and
 * only the ink is drawn). Runs on the renderer thread; the param is only a wake-up token.
 */
internal class InkRenderCallback(
    private val live: LiveStroke,
    private val viewport: ViewportState,
    private val style: () -> PageStyle,
    images: () -> PageImages? = { null },
    onSettled: () -> Unit = {},
    private val paper: () -> Boolean = { true },
    /** The committed ink is drawn in the window ([DryInkView]): the multi-buffered layer stays empty. */
    private val inkInWindow: () -> Boolean = { false },
) : CanvasFrontBufferedRenderer.Callback<Unit> {
    private val frontPainter = GeometryPainter(images, StampBuffer.HARDWARE_RING)
    private val dry = DryLayer(GeometryPainter(images), images, onSettled)
    private val dirty = RectF()
    private val next = DirtyRects()
    private var drawnPaint: FrontPaint? = null
    private var drawnView: ViewTransform? = null
    private var drawnStrokeId = NO_STROKE

    @Volatile
    private var cache: SceneRenderCache? = null

    /** Runs on the render thread when a multi-buffered frame is handed to the screen ([RedrawGate.landed]). */
    @Volatile
    var onLanded: () -> Unit = {}

    /** The front-buffered layer, once it has drawn: the hand-over to the window hides it ([FrontHandover]). */
    @Volatile
    var frontLayer: SurfaceControlCompat? = null
        private set

    /** The hand-over to the window's ink, whose hidden front layer the next stroke's first frame shows again. */
    @Volatile
    var handover: FrontHandover? = null

    /** The front frame being drawn starts a stroke on a layer a hand-over hid: show it with this frame. */
    private var showWithFrame = false

    /** Shows [scene] (or nothing); the next multi-buffered draw repaints the page from it. */
    fun attach(scene: PageScene?) {
        cache = scene?.let { SceneRenderCache(it) }
        dry.reset()
    }

    /** Stops the background render thread; the callback is not used afterwards. */
    fun release() = dry.release()

    /** Repaints the whole page on the next multi-buffered draw. */
    fun invalidate() = dry.invalidate()

    override fun onDrawFrontBufferedLayer(
        canvas: Canvas,
        bufferWidth: Int,
        bufferHeight: Int,
        param: Unit,
    ) {
        val frame = live.take() ?: return
        val view = viewport.transform
        if (frame.strokeId != drawnStrokeId || view != drawnView) {
            canvas.drawColor(Color.TRANSPARENT, BlendMode.CLEAR)
            if (frame.strokeId != drawnStrokeId && handover?.showWithFrame() == true) showWithFrame = true
            drawnStrokeId = frame.strokeId
            drawnView = view
            drawnPaint = null
        }
        frontPainter.view = view
        // Only what changed since the frame on the buffer is cleared and redrawn (see GeometryChange): clearing
        // and repainting the whole stroke every frame shows up as flashing lines on the single-buffered layer.
        next.clear()
        frame.paint.changedInto(drawnPaint, next)
        drawnPaint = frame.paint
        for (i in 0 until next.size) {
            screenBounds(view, next, i, dirty)
            canvas.withClip(dirty) {
                drawColor(Color.TRANSPARENT, BlendMode.CLEAR)
                frame.paint.draw(this, frontPainter)
            }
        }
    }

    /** The screen rect of page-space area [i] of [page], with room for anti-aliasing and round caps. */
    private fun screenBounds(
        view: ViewTransform,
        page: DirtyRects,
        i: Int,
        out: RectF,
    ) {
        out.set(
            view.toScreenX(page.left(i)) - EDGE_PX,
            view.toScreenY(page.top(i)) - EDGE_PX,
            view.toScreenX(page.right(i)) + EDGE_PX,
            view.toScreenY(page.bottom(i)) + EDGE_PX,
        )
    }

    override fun onDrawMultiBufferedLayer(
        canvas: Canvas,
        bufferWidth: Int,
        bufferHeight: Int,
        params: Collection<Unit>,
    ) {
        if (inkInWindow()) {
            // The window draws the committed ink; this layer only has to be empty.
            canvas.drawColor(Color.TRANSPARENT, BlendMode.CLEAR)
            return
        }
        dry.drawInto(canvas, cache, style(), viewport.transform, viewport.gesturing, paper())
    }

    override fun onFrontBufferedLayerRenderComplete(
        frontBufferedLayerSurfaceControl: SurfaceControlCompat,
        transaction: SurfaceControlCompat.Transaction,
    ) {
        frontLayer = frontBufferedLayerSurfaceControl
        if (showWithFrame) {
            showWithFrame = false
            transaction.setVisibility(frontBufferedLayerSurfaceControl, true)
        }
    }

    override fun onMultiBufferedLayerRenderComplete(
        frontBufferedLayerSurfaceControl: SurfaceControlCompat,
        multiBufferedLayerSurfaceControl: SurfaceControlCompat,
        transaction: SurfaceControlCompat.Transaction,
    ) = onLanded()

    private companion object {
        const val NO_STROKE = -1L
        const val EDGE_PX = 8f
    }
}
