package dev.penwise.frames

import android.annotation.SuppressLint
import android.content.Context
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import android.graphics.RectF
import android.view.View
import dev.penwise.ink.ViewTransform
import kotlin.math.ceil
import kotlin.math.max

/**
 * What a frame shows while it is not the live one (and outside the live window of the live one): the server's still,
 * strip by strip, at the frame's box on the board and carried by the camera like the paper. A frame without a still
 * shows a plain box with its route until one arrives. The WebView of the live frame is a sibling above this view.
 *
 * Draws only: the bitmaps come from [StillBitmaps], decoded elsewhere; a strip that is not decoded yet is a pale band.
 */
@SuppressLint("ViewConstructor") // built in code with its bitmaps, never inflated
internal class StillView(
    context: Context,
    private val bitmaps: StillBitmaps,
) : View(context) {
    private var specs: List<FrameSpec> = emptyList()
    private var manifests: Map<String, StillManifest> = emptyMap()
    private var transform = ViewTransform.IDENTITY

    private val density = resources.displayMetrics.density
    private val bitmapPaint = Paint(Paint.FILTER_BITMAP_FLAG)
    private val fillPaint = Paint().apply { color = PLACEHOLDER_FILL }
    private val strokePaint =
        Paint().apply {
            style = Paint.Style.STROKE
            color = OUTLINE
            strokeWidth = density
        }
    private val textPaint =
        Paint(Paint.ANTI_ALIAS_FLAG).apply {
            color = TEXT
            textSize = TEXT_SP * density
        }
    private val dst = RectF()

    /** Shows [next] frames with their [stills] under the camera [view]. */
    fun show(
        next: List<FrameSpec>,
        stills: Map<String, StillManifest>,
        view: ViewTransform,
    ) {
        specs = next
        manifests = stills
        transform = view
        invalidate()
    }

    override fun onDraw(canvas: Canvas) {
        for (index in specs.indices) {
            val spec = specs[index]
            val left = transform.toScreenX(spec.x)
            val top = transform.toScreenY(spec.y)
            val right = left + spec.cssWidth * transform.scale
            val bottom = top + spec.boxHeight * transform.scale
            if (right <= 0f || left >= width || bottom <= 0f || top >= height) continue
            val manifest = manifests[spec.id]
            if (manifest == null || manifest.strips.isEmpty()) {
                drawPlaceholder(canvas, spec, left, top, right, bottom)
            } else {
                drawStrips(canvas, spec, manifest, left, top)
            }
            dst.set(left, top, right, bottom)
            canvas.drawRect(dst, strokePaint)
        }
    }

    private fun drawPlaceholder(
        canvas: Canvas,
        spec: FrameSpec,
        left: Float,
        top: Float,
        right: Float,
        bottom: Float,
    ) {
        dst.set(left, top, right, bottom)
        canvas.drawRect(dst, fillPaint)
        val pad = PAD_DP * density
        val x = max(left, 0f) + pad
        val y = max(top, 0f) + pad + textPaint.textSize
        canvas.drawText(spec.route, x, y, textPaint)
    }

    /** The strips in view, each drawn or, until decoded, a pale band. No allocation: runs on every camera move. */
    private fun drawStrips(
        canvas: Canvas,
        spec: FrameSpec,
        manifest: StillManifest,
        left: Float,
        top: Float,
    ) {
        val scale = transform.scale
        val docTop = (0f - top) / scale
        val docBottom = (height - top) / scale
        val target = ceil(spec.cssWidth * scale).toInt()
        bitmapPaint.alpha = if (isDimmed(spec)) DIMMED_ALPHA else FULL_ALPHA
        val strips = manifest.strips
        for (index in strips.indices) {
            val strip = strips[index]
            if (!StillStrips.overlaps(strip, docTop, docBottom, MARGIN_CSS)) continue
            dst.set(left, top + strip.y * scale, left + spec.cssWidth * scale, top + (strip.y + strip.height) * scale)
            val bitmap = bitmaps.peek(strip.url)
            if (bitmap == null) {
                canvas.drawRect(dst, fillPaint)
            } else {
                canvas.drawBitmap(bitmap, null, dst, bitmapPaint)
            }
            bitmaps.request(strip, target)
        }
    }

    private companion object {
        val PLACEHOLDER_FILL = Color.argb(255, 244, 245, 247)
        val OUTLINE = Color.argb(255, 176, 184, 192)
        val TEXT = Color.argb(255, 84, 94, 104)
        const val TEXT_SP = 14f
        const val PAD_DP = 10f
        const val MARGIN_CSS = 256f
        const val DIMMED_ALPHA = 153
        const val FULL_ALPHA = 255
    }
}
