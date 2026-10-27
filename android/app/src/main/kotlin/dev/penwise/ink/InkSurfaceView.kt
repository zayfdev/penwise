package dev.penwise.ink

import android.content.Context
import android.util.AttributeSet
import android.view.InputDevice
import android.view.MotionEvent
import android.view.SurfaceHolder
import android.view.SurfaceView
import android.view.ViewConfiguration
import androidx.graphics.lowlatency.CanvasFrontBufferedRenderer
import androidx.input.motionprediction.MotionEventPredictor

/**
 * Full-screen drawing surface: stylus gestures go through [StylusStrokeController] into the core scene; the page is
 * drawn from the scene's render data by [InkRenderCallback], and the pen's in-progress frame goes through a
 * [CanvasFrontBufferedRenderer]. Fingers never draw; a two-finger tap undoes. Call [bind] before attaching.
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

        private val twoFingerTap =
            TwoFingerTap(ViewConfiguration.get(context).scaledTouchSlop * TAP_SLOP_FACTOR) { undo() }

        fun bind(
            scene: PageScene,
            brushes: BrushSource,
            images: PageImages? = null,
            liveOut: LiveOut? = null,
        ) {
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
            callback?.invalidate()
            redrawPage()
        }

        override fun onAttachedToWindow() {
            super.onAttachedToWindow()
            val current = scene ?: return
            requestUnbufferedDispatch(InputDevice.SOURCE_CLASS_POINTER)
            predictor = MotionEventPredictor.newInstance(this)
            val draw = InkRenderCallback(live, { pageStyle }, { images })
            callback = draw
            draw.attach(current)
            renderer = CanvasFrontBufferedRenderer(this, draw)
            controller = newController(current)
            holder.addCallback(this)
            current.onChanged = ::redrawPage
            images?.onChanged = {
                callback?.invalidate()
                redrawPage()
            }
        }

        override fun onDetachedFromWindow() {
            scene?.onChanged = null
            images?.onChanged = null
            holder.removeCallback(this)
            controller?.release()
            controller = null
            renderer?.release(true)
            renderer = null
            callback = null
            predictor = null
            super.onDetachedFromWindow()
        }

        override fun onTouchEvent(event: MotionEvent): Boolean {
            predictor?.record(event)
            val stylus = controller?.onTouchEvent(event) ?: false
            if (!stylus && controller?.isDrawing != true) twoFingerTap.onTouchEvent(event)
            if (event.actionMasked == MotionEvent.ACTION_UP && !stylus) performClick()
            return true
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
            brushes?.let { StylusStrokeController(page, it, live, this, liveOut, predictor) }

        private fun undo() {
            brushes?.selection?.clear()
            cancelStroke()
            scene?.undo()
        }

        private fun redrawPage() {
            renderer?.renderMultiBufferedLayer(REDRAW)
        }

        private companion object {
            const val TAP_SLOP_FACTOR = 2f
            val REDRAW = listOf(Unit)
        }
    }
