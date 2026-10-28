package dev.penwise.ink

import android.content.Context
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
 * tap undoes, one finger does nothing (it is usually the palm). Stylus samples are mapped screen → page before they
 * reach the core. Call [bind] before attaching.
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
            val draw = InkRenderCallback(live, viewport, { pageStyle }, { images }, { post(::redrawPage) })
            callback = draw
            draw.attach(current)
            renderer = CanvasFrontBufferedRenderer(this, draw)
            controller = newController(current)
            brushes?.selection?.onCleared = ::cancelStroke
            holder.addCallback(this)
            viewport.setViewSize(width.toFloat(), height.toFloat())
            viewport.setPage(pageStyle)
            viewport.onChanged = ::onViewChanged
            current.onChanged = ::redrawPage
            images?.onChanged = {
                callback?.invalidate()
                redrawPage()
            }
        }

        override fun onDetachedFromWindow() {
            scene?.onChanged = null
            images?.onChanged = null
            viewport.onChanged = null
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
            fingers.onTouchEvent(event, stylus || pen.drawing || penNear(event.eventTime))
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
