package dev.penwise.ink

import android.content.Context
import android.util.AttributeSet
import android.view.InputDevice
import android.view.MotionEvent
import android.view.SurfaceHolder
import android.view.SurfaceView
import android.view.ViewConfiguration
import androidx.graphics.lowlatency.CanvasFrontBufferedRenderer

/**
 * Full-screen drawing surface: stylus strokes go through [StylusStrokeController] into a
 * [CanvasFrontBufferedRenderer]; fingers never draw, a two-finger tap undoes the last stroke.
 * Call [bind] once before the view is attached.
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
        private var store: StrokeStore? = null
        private var controller: StylusStrokeController? = null
        private var renderer: CanvasFrontBufferedRenderer<Unit>? = null
        private val twoFingerTap =
            TwoFingerTap(ViewConfiguration.get(context).scaledTouchSlop * TAP_SLOP_FACTOR) { store?.undo() }

        fun bind(
            store: StrokeStore,
            brushes: BrushSource,
            liveOut: LiveOut? = null,
        ) {
            this.store = store
            controller = StylusStrokeController(store, brushes, live, this, liveOut)
        }

        override fun onAttachedToWindow() {
            super.onAttachedToWindow()
            val pageStore = store ?: return
            requestUnbufferedDispatch(InputDevice.SOURCE_CLASS_POINTER)
            renderer = CanvasFrontBufferedRenderer(this, InkRenderCallback(pageStore, live))
            holder.addCallback(this)
            pageStore.onPageChanged = ::redrawPage
        }

        override fun onDetachedFromWindow() {
            store?.onPageChanged = null
            holder.removeCallback(this)
            controller?.release()
            renderer?.release(true)
            renderer = null
            super.onDetachedFromWindow()
        }

        override fun onTouchEvent(event: MotionEvent): Boolean {
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
            redrawPage()
        }

        override fun surfaceDestroyed(holder: SurfaceHolder) = Unit

        private fun redrawPage() {
            renderer?.renderMultiBufferedLayer(REDRAW)
        }

        private companion object {
            const val TAP_SLOP_FACTOR = 2f
            val REDRAW = listOf(Unit)
        }
    }
