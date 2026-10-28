package dev.penwise.ink

import android.graphics.Bitmap
import android.graphics.BlendMode
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import androidx.core.graphics.createBitmap
import java.util.concurrent.atomic.AtomicBoolean

/**
 * The committed page as one persistent screen-sized bitmap (paper, pictures, ink) rendered through the view
 * transform: a commit draws only the new item into it, an erase or undo repaints only the area of the items that
 * changed, and the multi-buffered layer is a single `drawBitmap`.
 *
 * Panning and zooming never re-render on this thread: while the view differs from the one the bitmap was rendered
 * under, the bitmap is drawn through the changing transform (one `drawBitmap` per frame; whatever it does not cover
 * shows the paper colour). Once the gesture settles, a [SettleRenderer] draws the visible page at the new view off
 * this thread and the finished bitmap is swapped in whole; a render the view overtook is dropped, one the scene
 * overtook gets the changed area repainted onto it ([SettleLedger]). A new gesture cancels it, never waits for it.
 *
 * A bitmap that was drawn is never painted into again: the hardware canvas records `drawBitmap` and reads the pixels
 * when it replays, a frame later. Paints go into a spare bitmap that then replaces the shown one, and a replaced
 * bitmap rests in a [CoolingRack] before it is reused. Render thread only; [invalidate] and [reset] are the
 * entry points for other threads.
 */
internal class DryLayer(
    painter: GeometryPainter,
    images: () -> PageImages? = { null },
    onSettled: () -> Unit = {},
) {
    private var bitmap: Bitmap? = null
    private var canvas: Canvas? = null

    /** The bitmap has been handed to a `drawBitmap` (so it may still be read by a replay) and must not be painted. */
    private var exposed = false
    private var rendered: ViewTransform? = null

    /** The scene changed while the bitmap showed another view: it no longer holds all of it. */
    private var pendingFull = false
    private val ledger = SettleLedger()
    private val pagePainter = PagePainter(painter)
    private val scaled = ScaledDraw()
    private val copyPaint = Paint().apply { blendMode = BlendMode.SRC }
    private val replacePaint = Paint().apply { blendMode = BlendMode.SRC }

    /** Draw the paper, or the ink alone on a transparent bitmap (a live page shows through the surface). */
    private var paper = true
    private val stale = AtomicBoolean(true)
    private val discard = AtomicBoolean(false)
    private val settler = SettleRenderer(images, onSettled)
    private val cooling = CoolingRack { settler.recycle(it) }

    /** Marks the whole layer for repaint (paper style, picture arrival, resize). */
    fun invalidate() {
        stale.set(true)
    }

    /** Forgets what the bitmap shows (page switch): the next frame renders the new page in place, not scaled. */
    fun reset() {
        discard.set(true)
        stale.set(true)
    }

    fun release() {
        settler.release()
    }

    /**
     * Brings the bitmap in line with the scene, the paper and the [view], then draws it onto [target]. While the view
     * is away from the one the bitmap shows, the bitmap is drawn transformed and, once [gesturing] is over, a render
     * for the new view is started in the background. A closed scene counts as no scene.
     */
    fun drawInto(
        target: Canvas,
        cache: SceneRenderCache?,
        style: PageStyle,
        view: ViewTransform,
        gesturing: Boolean,
        paper: Boolean = true,
    ) {
        cooling.tick()
        val full = takeFullRepaint(target.width, target.height, paper)
        val plan = cache?.takeUnless { it.closed }?.update() ?: RenderPlan.Idle
        val live = cache?.takeUnless { it.closed }
        val changed = full || plan !is RenderPlan.Idle
        if (changed) ledger.sceneChanged(plan, full)
        val shown = rendered
        // A finished background render already shows this scene state; otherwise bring the bitmap up to date here.
        if (!swapSettled(live, style, view)) {
            if (shown != null && shown != view) {
                pendingFull = pendingFull || changed
                if (gesturing) cancelSettle() else settle(live, style, view, target)
                // What the old bitmap does not cover shows the backdrop (or the live page, under a frame).
                if (paper) {
                    target.drawColor(PaperPainter.backdrop(style))
                } else {
                    target.drawColor(Color.TRANSPARENT, BlendMode.CLEAR)
                }
                bitmap?.let {
                    scaled.draw(target, it, shown, view)
                    exposed = true
                }
                return
            }
            cancelSettle()
            paint(live, style, view, plan, full || shown == null || pendingFull)
            pendingFull = false
            rendered = view
        }
        bitmap?.let {
            // Over a live page the bitmap (which covers the whole surface) replaces the buffer: an ink-only layer
            // must not be drawn over whatever the last multi-buffered frame left there.
            target.drawBitmap(it, 0f, 0f, replacePaint.takeUnless { paper })
            exposed = true
        }
    }

    private fun cancelSettle() {
        settler.cancel()
        ledger.clear()
    }

    /** Sizes the bitmap and forgets stale content; true when the whole layer needs painting. */
    private fun takeFullRepaint(
        width: Int,
        height: Int,
        paper: Boolean,
    ): Boolean {
        if (paper != this.paper) {
            // Turning the paper off (or back on) reinvents every pixel: forget this bitmap and any render for it.
            this.paper = paper
            reset()
        }
        val resized = ensureBitmap(width, height)
        if (discard.getAndSet(false) || resized) {
            rendered = null
            cancelSettle()
        }
        return stale.getAndSet(false) || resized
    }

    /** Paints [plan] (or everything) into a bitmap nobody is reading and makes it the shown one. */
    private fun paint(
        cache: SceneRenderCache?,
        style: PageStyle,
        view: ViewTransform,
        plan: RenderPlan,
        everything: Boolean,
    ) {
        if (!everything && plan is RenderPlan.Idle) return
        val shownBitmap = bitmap ?: return
        if (exposed) {
            val spare = settler.acquire(shownBitmap.width, shownBitmap.height)
            // A full repaint overwrites every pixel (paper first); a partial one starts from the shown content.
            if (!everything) Canvas(spare).drawBitmap(shownBitmap, 0f, 0f, copyPaint)
            install(spare)
        }
        canvas?.let { pagePainter.paint(it, cache, style, view, plan, everything, paper) }
    }

    /** Takes a finished background render in when it is for this view, repainting what the scene changed since. */
    private fun swapSettled(
        cache: SceneRenderCache?,
        style: PageStyle,
        view: ViewTransform,
    ): Boolean {
        val done = settler.take() ?: return false
        val current = bitmap
        val patch =
            if (current != null && done.bitmap.width == current.width && done.bitmap.height == current.height) {
                ledger.accept(done.view, view, style)
            } else {
                null
            }
        if (patch == null) {
            settler.recycle(done.bitmap)
            return false
        }
        // The render was never shown, so it can be painted into here.
        if (!patch.isEmpty) {
            pagePainter.paint(Canvas(done.bitmap), cache, style, view, RenderPlan.Region(patch), false, paper)
        }
        install(done.bitmap)
        rendered = view
        pendingFull = false
        return true
    }

    /** Makes [next] the shown bitmap; the old one may still be replaying a draw, so it rests before reuse. */
    private fun install(next: Bitmap) {
        bitmap?.let { cooling.add(it) }
        bitmap = next
        canvas = Canvas(next)
        exposed = false
    }

    /** Starts (or keeps) the background render for [view]; a request for another view or style is superseded. */
    private fun settle(
        cache: SceneRenderCache?,
        style: PageStyle,
        view: ViewTransform,
        target: Canvas,
    ) {
        if (settler.pending && ledger.covers(view, style)) return
        ledger.submitted(view, style)
        settler.request(
            SettleRequest(
                view,
                style,
                target.width,
                target.height,
                cache?.paintOrder ?: emptyList(),
                cache?.snapshotItems() ?: emptyMap(),
                paper,
            ),
        )
    }

    /** Creates the bitmap for a new size; true when it is new (and so blank). */
    private fun ensureBitmap(
        width: Int,
        height: Int,
    ): Boolean {
        val resized = bitmap?.width != width || bitmap?.height != height
        if (resized) {
            install(createBitmap(width, height, Bitmap.Config.ARGB_8888))
        }
        return resized
    }
}
