package dev.penwise.ink

import android.content.Context
import android.graphics.PixelFormat
import android.util.AttributeSet
import android.view.InputDevice
import android.view.MotionEvent
import android.view.PointerIcon
import android.view.SurfaceHolder
import android.view.SurfaceView
import android.view.ViewConfiguration
import androidx.graphics.lowlatency.CanvasFrontBufferedRenderer
import androidx.input.motionprediction.MotionEventPredictor

/**
 * Full-screen drawing surface: stylus gestures go through [StylusStrokeController] into the core scene; the page is
 * drawn from the scene's render data by [InkRenderCallback], and the pen's in-progress frame goes through a
 * [CanvasFrontBufferedRenderer]. Fingers never draw: two fingers pan and pinch-zoom the [ViewportState], a two-finger
 * tap undoes, one finger does nothing (it is usually the palm) — unless live pages sit under the canvas ([frames]):
 * then one finger on a page is the page's, and the surface composites above the window with a translucent, ink-only
 * layer so the pages show through. Stylus samples are mapped screen → page before they reach the core. Call [bind]
 * before attaching.
 */
class InkSurfaceView
    @JvmOverloads
    constructor(
        context: Context,
        attrs: AttributeSet? = null,
    ) : SurfaceView(context, attrs),
        StrokeSink,
        SurfaceHolder.Callback {
        private val live = LiveStroke()
        private var scene: PageScene? = null
        private var brushes: BrushSource? = null
        private var images: PageImages? = null
        private var liveOut: LiveOut? = null
        private var controller: StylusStrokeController? = null
        private var renderer: CanvasFrontBufferedRenderer<Unit>? = null
        private var callback: InkRenderCallback? = null
        private var predictor: MotionEventPredictor? = null

        @Volatile
        private var pageStyle: PageStyle = PageStyle.BLANK

        private var viewport = ViewportState()
        private var pen = PenActivity()
        private var frames: FrameHost? = null

        /** The surface is above the window and carries ink only: live pages are drawn by the window below it. */
        private var overFrames = false
        private val viewListener: (ViewTransform) -> Unit = { onViewChanged() }

        /** Runs on the UI thread whenever the stylus touches down (the canvas is about to be written on). */
        var onPenDown: () -> Unit = {}

        private val fingers =
            FingerRouter(
                FingerNavigation(
                    ViewConfiguration.get(context).scaledTouchSlop * TAP_SLOP_FACTOR,
                    onTap = ::undo,
                    onStep = { viewport.transformBy(it.factor, it.fromX, it.fromY, it.toX, it.toY) },
                    onActive = { viewport.setGesturing(it) },
                ),
            )

        fun bind(
            scene: PageScene,
            brushes: BrushSource,
            viewport: ViewportState,
            images: PageImages? = null,
            liveOut: LiveOut? = null,
            pen: PenActivity = PenActivity(),
        ) {
            this.pen = pen
            this.viewport = viewport
            this.brushes = brushes
            this.images = images
            this.liveOut = liveOut
            showScene(scene)
        }

        /** Switches to another page's scene: abandons the gesture in flight and repaints from the new scene. */
        fun showScene(next: PageScene) {
            if (scene === next) return
            controller?.release()
            brushes?.selection?.clear()
            pen.drawing = false
            scene?.onChanged = null
            scene = next
            callback?.attach(next)
            controller = newController(next)
            if (isAttachedToWindow) next.onChanged = ::redrawPage
            redrawPage()
        }

        /**
         * Puts live pages under the ink, or takes them away. With frames the surface goes above the window
         * (`setZOrderOnTop`) and becomes translucent with the ink alone drawn on it, so the pages show through; without
         * them the canvas keeps today's opaque surface behind the window (the fast path). [FrameHost] is usually the
         * frames' own layer.
         */
        internal fun setFrames(host: FrameHost?) {
            if (host === frames) return
            frames?.cancel()
            frames = host
            setOverlay(host != null)
        }

        /** Sets the paper drawn under the ink; repaints when it changes. */
        fun setPageStyle(style: PageStyle) {
            if (pageStyle == style) return
            pageStyle = style
            viewport.setPage(style)
            callback?.invalidate()
            redrawPage()
        }

        override fun onAttachedToWindow() {
            super.onAttachedToWindow()
            val current = scene ?: return
            requestUnbufferedDispatch(InputDevice.SOURCE_CLASS_POINTER)
            predictor = MotionEventPredictor.newInstance(this)
            val draw =
                InkRenderCallback(live, viewport, { pageStyle }, { images }, { post(::redrawPage) }, { !overFrames })
            callback = draw
            draw.attach(current)
            renderer = CanvasFrontBufferedRenderer(this, draw)
            controller = newController(current)
            brushes?.selection?.onCleared = ::cancelStroke
            holder.addCallback(this)
            viewport.setViewSize(width.toFloat(), height.toFloat())
            viewport.setPage(pageStyle)
            viewport.observers.add(viewListener)
            current.onChanged = ::redrawPage
            images?.onChanged = {
                callback?.invalidate()
                redrawPage()
            }
        }

        override fun onDetachedFromWindow() {
            scene?.onChanged = null
            images?.onChanged = null
            viewport.observers.remove(viewListener)
            fingers.cancel()
            viewport.setGesturing(false)
            holder.removeCallback(this)
            controller?.release()
            controller = null
            brushes?.selection?.onCleared = null
            pen.drawing = false
            renderer?.release(true)
            renderer = null
            callback?.release()
            callback = null
            predictor = null
            super.onDetachedFromWindow()
        }

        override fun onSizeChanged(
            w: Int,
            h: Int,
            oldw: Int,
            oldh: Int,
        ) {
            super.onSizeChanged(w, h, oldw, oldh)
            viewport.setViewSize(w.toFloat(), h.toFloat())
            if (controller?.isDrawing == true && (w != oldw || h != oldh)) {
                // Samples were mapped with the old size; a stroke continued across a resize would be displaced.
                controller?.abandon()
                cancelStroke()
                pen.drawing = false
            }
        }

        override fun onTouchEvent(event: MotionEvent): Boolean {
            predictor?.record(event)
            val stylus = controller?.onTouchEvent(event) ?: false
            if (stylus) {
                pen.nearUntil = event.eventTime + HOVER_HOLD_MS
                if (isPenDown(event)) onPenDown()
            }
            pen.drawing = controller?.isDrawing == true
            val busy = stylus || pen.drawing || penNear(event.eventTime)
            val host = frames
            // The pen wins over a page: a gesture on it is abandoned so no stray click survives the stroke.
            if (busy) host?.cancel()
            val onPage = !busy && host != null && host.route(event) { fingers.adopt(it) }
            if (!onPage) fingers.onTouchEvent(event, busy)
            if (event.actionMasked == MotionEvent.ACTION_UP && !stylus) performClick()
            return true
        }

        /** A hovering stylus means a hand is about to write: fingers (a resting palm) must not pan, zoom or undo. */
        override fun onHoverEvent(event: MotionEvent): Boolean {
            val tool = event.getToolType(0)
            if (tool == MotionEvent.TOOL_TYPE_STYLUS || tool == MotionEvent.TOOL_TYPE_ERASER) {
                val exit = event.actionMasked == MotionEvent.ACTION_HOVER_EXIT
                pen.nearUntil = if (exit) 0L else event.eventTime + HOVER_HOLD_MS
                // Claiming the hover is what makes the system send HOVER_EXIT when the pen leaves.
                return true
            }
            return super.onHoverEvent(event)
        }

        /**
         * The system draws a hover dot (stylus pointer icon) under a hovering pen; over the page it only sits on the
         * ink being written, so the pen and the eraser get the null icon. Mice and other pointers keep theirs.
         */
        override fun onResolvePointerIcon(
            event: MotionEvent,
            pointerIndex: Int,
        ): PointerIcon? {
            val tool = event.getToolType(pointerIndex)
            if (tool == MotionEvent.TOOL_TYPE_STYLUS || tool == MotionEvent.TOOL_TYPE_ERASER) {
                return PointerIcon.getSystemIcon(context, PointerIcon.TYPE_NULL)
            }
            return super.onResolvePointerIcon(event, pointerIndex)
        }

        /** Switches the surface between the opaque page (no frames) and an ink-only overlay above the window. */
        private fun setOverlay(over: Boolean) {
            if (over == overFrames) return
            overFrames = over
            // Above the window is the only place the ink can be while WebViews live in the window; TRANSLUCENT keeps
            // the punch-out from being an opaque black sheet between the ink and the pages.
            setZOrderOnTop(over)
            holder.setFormat(if (over) PixelFormat.TRANSLUCENT else PixelFormat.OPAQUE)
            redrawPage()
        }

        private fun penNear(now: Long): Boolean = now < pen.nearUntil

        private fun isPenDown(event: MotionEvent): Boolean {
            val action = event.actionMasked
            if (action != MotionEvent.ACTION_DOWN && action != MotionEvent.ACTION_POINTER_DOWN) return false
            val tool = event.getToolType(event.actionIndex)
            return tool == MotionEvent.TOOL_TYPE_STYLUS || tool == MotionEvent.TOOL_TYPE_ERASER
        }

        override fun performClick(): Boolean = super.performClick()

        override fun requestFrontFrame() {
            renderer?.renderFrontBufferedLayer(Unit)
        }

        override fun commitStroke() {
            renderer?.commit()
        }

        override fun cancelStroke() {
            renderer?.cancel()
        }

        override fun surfaceCreated(holder: SurfaceHolder) = Unit

        /** The renderer recreates its buffers on resize; repaint the page into the new multi-buffered layer. */
        override fun surfaceChanged(
            holder: SurfaceHolder,
            format: Int,
            width: Int,
            height: Int,
        ) {
            callback?.invalidate()
            redrawPage()
        }

        override fun surfaceDestroyed(holder: SurfaceHolder) = Unit

        private fun newController(page: PageScene): StylusStrokeController? =
            brushes?.let { StylusStrokeController(page, it, live, this, viewport, liveOut, predictor) }

        private fun undo() {
            brushes?.selection?.clear()
            cancelStroke()
            scene?.undo()
        }

        private fun redrawPage() {
            renderer?.renderMultiBufferedLayer(REDRAW)
        }

        /** The view moved: repaint the page, and the selection frame (it lives on the front layer in page space). */
        private fun onViewChanged() {
            redrawPage()
            if ((brushes?.selection?.count ?: 0) > 0) requestFrontFrame()
        }

        private companion object {
            const val TAP_SLOP_FACTOR = 1.5f

            /**
             * How long a stylus that stopped reporting hover still counts as near. Hover events arrive every frame
             * while the pen is in range, so this only bridges touch-up to the next hover event; a longer hold made
             * the two-finger undo dead right after writing.
             */
            const val HOVER_HOLD_MS = 250L
            val REDRAW = listOf(Unit)
        }
    }
