package dev.penwise.ink

import android.content.Context
import android.graphics.Canvas
import android.util.AttributeSet
import android.view.View

/**
 * The paper, drawn in the window instead of on the ink surface.
 *
 * While live pages sit under the canvas (frames) the surface is translucent and carries the ink alone, so it can be
 * above the WebViews; the sheet, its ruling and the desk around it then come from this ordinary view, which sits below
 * the frames. It follows the canvas [ViewportState] like everything else. Without frames the surface keeps drawing the
 * paper itself (the opaque fast path) and this view is not used.
 */
internal class PaperView
    @JvmOverloads
    constructor(
        context: Context,
        attrs: AttributeSet? = null,
    ) : View(context, attrs) {
        private var viewport: ViewportState? = null
        private var style = PageStyle.BLANK
        private val repaint: (ViewTransform) -> Unit = { invalidate() }

        /** Shows [style]'s paper through [viewport]. */
        fun bind(
            viewport: ViewportState,
            style: PageStyle,
        ) {
            if (this.viewport !== viewport) {
                detach()
                this.viewport = viewport
                observe()
            }
            if (this.style == style) return
            this.style = style
            invalidate()
        }

        override fun onAttachedToWindow() {
            super.onAttachedToWindow()
            observe()
        }

        override fun onDetachedFromWindow() {
            detach()
            super.onDetachedFromWindow()
        }

        override fun onDraw(canvas: Canvas) {
            PaperPainter.draw(canvas, style, viewport?.transform ?: ViewTransform.IDENTITY)
        }

        private fun observe() {
            if (!isAttachedToWindow) return
            viewport?.observers?.add(repaint)
        }

        private fun detach() {
            viewport?.observers?.remove(repaint)
        }
    }
