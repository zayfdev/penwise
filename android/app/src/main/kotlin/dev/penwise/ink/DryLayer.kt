package dev.penwise.ink

import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Matrix
import android.graphics.Paint
import androidx.core.graphics.createBitmap
import androidx.core.graphics.withMatrix
import java.util.concurrent.atomic.AtomicBoolean

/**
 * The committed page as one persistent screen-sized bitmap (paper, pictures, ink) rendered through the view
 * transform: a commit draws only the new item into it, an erase or undo repaints only the area of the items that
 * changed, and the multi-buffered layer is a single `drawBitmap`.
 *
 * Panning and zooming never re-render on this thread: while the view differs from the one the bitmap was rendered
 * under, the bitmap is drawn through the changing transform (one `drawBitmap` per frame; whatever it does not cover
 * shows the paper colour). Once the gesture settles, a [SettleRenderer] draws the visible page at the new view off
 * this thread and the finished bitmap is swapped in whole; a render that the view or the scene overtook is dropped. A
 * new gesture cancels it, never waits for it. Render thread only; [invalidate] and [reset] are the entry points for
 * other threads.
 */
internal class DryLayer(
    painter: GeometryPainter,
    images: () -> PageImages? = { null },
    onSettled: () -> Unit = {},
) {
    private var bitmap: Bitmap? = null
    private var canvas: Canvas? = null
    private var rendered: ViewTransform? = null

    /** The scene changed while the bitmap showed another view: it no longer holds all of it. */
    private var pendingFull = false
    private var epoch = 0L
    private var requested: SettleRequest? = null
    private val pagePainter = PagePainter(painter)
    private val matrix = Matrix()
    private val bitmapPaint = Paint(Paint.FILTER_BITMAP_FLAG)
    private val stale = AtomicBoolean(true)
    private val discard = AtomicBoolean(false)
    private val settler = SettleRenderer(images, onSettled)

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
     * for the new view is started in the background.
     */
    fun drawInto(
        target: Canvas,
        cache: SceneRenderCache?,
        style: PageStyle,
        view: ViewTransform,
        gesturing: Boolean,
    ) {
        val full = takeFullRepaint(target.width, target.height)
        val plan = cache?.update() ?: RenderPlan.Idle
        val changed = full || plan !is RenderPlan.Idle
        if (changed) epoch++
        val shown = rendered
        // A finished background render already shows this scene state; otherwise bring the bitmap up to date here.
        if (!swapSettled(view)) {
            if (shown != null && shown != view) {
                pendingFull = pendingFull || changed
                if (gesturing) settler.cancel() else settle(cache, style, view, target)
                drawScaled(target, style, shown, view)
                return
            }
            settler.cancel()
            canvas?.let { pagePainter.paint(it, cache, style, view, plan, full || shown == null) }
            pendingFull = false
            rendered = view
        }
        bitmap?.let { target.drawBitmap(it, 0f, 0f, null) }
    }

    /** Sizes the bitmap and forgets stale content; true when the whole layer needs painting. */
    private fun takeFullRepaint(
        width: Int,
        height: Int,
    ): Boolean {
        val resized = ensureBitmap(width, height)
        if (discard.getAndSet(false) || resized) {
            rendered = null
            settler.cancel()
        }
        return stale.getAndSet(false) || resized || pendingFull
    }

    /** Takes a finished background render in when it is for exactly this view and this scene state. */
    private fun swapSettled(view: ViewTransform): Boolean {
        val done = settler.take() ?: return false
        val current = bitmap
        if (current == null || !done.matches(view, epoch, current.width, current.height)) {
            settler.recycle(done.bitmap)
            return false
        }
        bitmap = done.bitmap
        canvas = Canvas(done.bitmap)
        settler.recycle(current)
        rendered = view
        pendingFull = false
        requested = null
        return true
    }

    /** Starts (or keeps) the background render for [view]; a request for another view or scene state is superseded. */
    private fun settle(
        cache: SceneRenderCache?,
        style: PageStyle,
        view: ViewTransform,
        target: Canvas,
    ) {
        val open = requested
        if (settler.pending && open != null && open.covers(view, epoch, style)) return
        val request =
            SettleRequest(
                view,
                style,
                target.width,
                target.height,
                cache?.paintOrder ?: emptyList(),
                cache?.snapshotItems() ?: emptyMap(),
                epoch,
            )
        requested = request
        settler.request(request)
    }

    /** Creates the bitmap for a new size; true when it is new (and so blank). */
    private fun ensureBitmap(
        width: Int,
        height: Int,
    ): Boolean {
        val resized = bitmap?.width != width || bitmap?.height != height
        if (resized) {
            bitmap?.recycle()
            bitmap = createBitmap(width, height, Bitmap.Config.ARGB_8888)
            canvas = Canvas(requireNotNull(bitmap))
        }
        return resized
    }

    /** Draws the bitmap rendered under [from] as it looks under [to]; what it does not cover shows the backdrop. */
    private fun drawScaled(
        target: Canvas,
        style: PageStyle,
        from: ViewTransform,
        to: ViewTransform,
    ) {
        target.drawColor(PaperPainter.backdrop(style))
        val k = to.scale / from.scale
        matrix.setScale(k, k)
        matrix.postTranslate(to.tx - k * from.tx, to.ty - k * from.ty)
        bitmap?.let { target.withMatrix(matrix) { drawBitmap(it, 0f, 0f, bitmapPaint) } }
    }
}
