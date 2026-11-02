package dev.penwise.ink

import android.content.Context
import android.graphics.Canvas
import android.graphics.RenderNode
import android.view.View
import androidx.core.graphics.withSave

/**
 * The committed ink while live pages sit under the canvas, drawn by an ordinary view in the window above the pages
 * (and below the pen's low-latency surface, which keeps only the stroke being written). Page and ink are then in the
 * same window frame: a scroll moves both in the same frame, never one a frame before the other, which a separate
 * surface could not promise (the owner saw ink trail or lead the page by a frame, up to 75 px in a fling).
 *
 * The ink is recorded as a display list under one camera view for the screen and one screen around it
 * ([DryInkPlan]), and drawn every frame through the camera's current transform: a scroll costs a matrix, not a
 * repaint. The list is recorded again when the scene changes, when the screen leaves the band, and when the zoom
 * changed and the camera rests. UI thread only; [sceneChanged] and [reset] may be called from any thread.
 */
internal class DryInkView(
    context: Context,
) : View(context) {
    private var viewport: ViewportState? = null
    private var scene: PageScene? = null
    private var cache: SceneRenderCache? = null
    private var images: PageImages? = null
    private val painter = GeometryPainter({ images }, StampBuffer.RECORDED)
    private val node = RenderNode("penwise-dry-ink").apply { setClipToBounds(false) }
    private var recorded: RecordedInk? = null

    @Volatile
    private var stale = true
    private val follow: (ViewTransform) -> Unit = { invalidate() }

    /** Shows [scene] (or nothing) through [viewport]; a new scene is read whole at the next frame. */
    fun bind(
        scene: PageScene?,
        images: PageImages?,
        viewport: ViewportState,
    ) {
        if (scene !== this.scene) {
            this.scene = scene
            cache = scene?.let { SceneRenderCache(it) }
            stale = true
        }
        this.images = images
        if (viewport !== this.viewport) {
            this.viewport?.observers?.remove(follow)
            this.viewport = viewport
            if (isAttachedToWindow) viewport.observers.add(follow)
        }
        invalidate()
    }

    /** The scene changed (any thread): the next frame reads what changed. */
    fun sceneChanged() = postInvalidateOnAnimation()

    /** Something the scene's dirty list cannot say changed (a picture arrived): record everything again. */
    fun reset() {
        stale = true
        postInvalidateOnAnimation()
    }

    override fun onAttachedToWindow() {
        super.onAttachedToWindow()
        viewport?.observers?.add(follow)
    }

    override fun onDetachedFromWindow() {
        viewport?.observers?.remove(follow)
        super.onDetachedFromWindow()
    }

    override fun onSizeChanged(
        w: Int,
        h: Int,
        oldw: Int,
        oldh: Int,
    ) {
        super.onSizeChanged(w, h, oldw, oldh)
        stale = true
    }

    override fun onDraw(canvas: Canvas) {
        val camera = viewport ?: return
        val view = camera.transform
        val live = cache?.takeUnless { it.closed }
        val plan = live?.update() ?: RenderPlan.Idle
        val changed = plan !is RenderPlan.Idle || stale
        stale = false
        if (!canvas.isHardwareAccelerated) {
            // A software canvas (a test, a screenshot of the view): draw the visible ink straight away.
            draw(canvas, live, view, DryInkPlan.band(view, width, height))
            return
        }
        val moving = camera.gesturing || camera.animating
        if (DryInkPlan.needsRecord(recorded, view, width, height, changed, moving)) record(live, view)
        val shown = recorded ?: return
        val shift = DryInkPlan.shift(shown.view, view)
        canvas.withSave {
            translate(shift.dx, shift.dy)
            scale(shift.scale, shift.scale)
            drawRenderNode(node)
        }
    }

    /** Records the ink of the band around [view]; the node spans the screen and one screen on every side. */
    private fun record(
        cache: SceneRenderCache?,
        view: ViewTransform,
    ) {
        val (w, h) = width to height
        if (w <= 0 || h <= 0) return
        val band = DryInkPlan.band(view, w, h)
        node.setPosition(-w, -h, 2 * w, 2 * h)
        val recording = node.beginRecording(BAND_SCREENS * w, BAND_SCREENS * h)
        try {
            recording.translate(w.toFloat(), h.toFloat())
            draw(recording, cache, view, band)
        } finally {
            node.endRecording()
        }
        recorded = RecordedInk(view, band)
    }

    private fun draw(
        canvas: Canvas,
        cache: SceneRenderCache?,
        view: ViewTransform,
        band: Bounds,
    ) {
        cache ?: return
        painter.view = view
        for (key in cache.paintOrder) {
            val item = cache[key] ?: continue
            if (item.bounds.intersects(band)) painter.draw(canvas, item)
        }
    }

    private companion object {
        /** The recorded band is three screens wide and high: the screen and one on every side. */
        const val BAND_SCREENS = 3
    }
}
