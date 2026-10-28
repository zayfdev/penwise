package dev.penwise.ink

import android.graphics.BlendMode
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Matrix
import android.graphics.Paint
import android.graphics.Path
import android.graphics.RectF
import android.text.Layout
import android.text.StaticLayout
import android.text.TextPaint
import androidx.core.graphics.withMatrix
import androidx.core.graphics.withTranslation
import dev.penwise.core.InkGeometry

/**
 * Draws render data on a Canvas in *screen* space: geometry arrives in page space and is carried to the screen by
 * [view] (set it before drawing). Vector parts go through the canvas matrix; stamp strokes are rasterised straight
 * at screen resolution, so they stay crisp at any zoom. Not thread-safe: one instance per render layer.
 */
internal class GeometryPainter(
    private val images: () -> PageImages? = { null },
) {
    private val path = Path()
    private val bounds = RectF()
    private val fillPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply { style = Paint.Style.FILL }
    private val stampPaint = Paint()
    private val layerPaint = Paint()
    private val stampBuffer = StampBuffer()
    private val maskPaint =
        Paint(Paint.ANTI_ALIAS_FLAG).apply {
            style = Paint.Style.STROKE
            strokeCap = Paint.Cap.ROUND
            strokeJoin = Paint.Join.ROUND
            blendMode = BlendMode.DST_OUT
        }
    private val textPaint = TextPaint(Paint.ANTI_ALIAS_FLAG)
    private val imagePaint = Paint(Paint.FILTER_BITMAP_FLAG)
    private val matrix = Matrix()
    private val viewMatrix = Matrix()
    private val screenBounds = RectF()
    private val src = FloatArray(POLY)
    private val dst = FloatArray(POLY)

    /** Page → screen mapping for everything drawn next. */
    var view: ViewTransform = ViewTransform.IDENTITY
        set(value) {
            field = value
            viewMatrix.setScale(value.scale, value.scale)
            viewMatrix.postTranslate(value.tx, value.ty)
        }

    /** The in-progress stroke straight from the ink engine (front buffer). */
    fun draw(
        canvas: Canvas,
        geometry: InkGeometry,
    ) {
        if (geometry.isStamps) {
            val grain = GrainStyle(geometry.grainScale, geometry.grainSoftness)
            stamps(canvas, geometry.dabs, geometry.argb, geometry.opacity, geometry.multiply, grain)
        } else {
            val (points, ends) = geometry.outline to geometry.contourEnds
            outline(canvas, points, ends, geometry.argb, geometry.opacity, false, geometry.multiply, null)
        }
    }

    /** One committed item: its parts in order, then its erase masks cut out of the item alone. */
    fun draw(
        canvas: Canvas,
        item: ItemRender,
    ) {
        val masks = item.masks
        if (masks.isEmpty()) {
            item.parts.forEach { part(canvas, it, item.multiply) }
            return
        }
        bounds.set(item.bounds.left, item.bounds.top, item.bounds.right, item.bounds.bottom)
        viewMatrix.mapRect(screenBounds, bounds)
        screenBounds.inset(-LAYER_PAD_PX, -LAYER_PAD_PX)
        layerPaint.alpha = FULL_ALPHA
        layerPaint.blendMode = if (item.multiply) BlendMode.MULTIPLY else null
        val save = canvas.saveLayer(screenBounds, layerPaint)
        item.parts.forEach { if (it !is RenderPart.Mask) part(canvas, it, false) }
        canvas.withMatrix(viewMatrix) { masks.forEach { mask(this, it) } }
        canvas.restoreToCount(save)
    }

    private fun part(
        canvas: Canvas,
        part: RenderPart,
        multiply: Boolean,
    ) {
        when (part) {
            is RenderPart.Outline -> {
                val blend = multiply && !part.normalBlend
                outline(canvas, part.points, part.contourEnds, part.argb, part.opacity, part.evenOdd, blend, part)
            }

            is RenderPart.Stamps -> {
                val grain = GrainStyle(part.grainScale, part.grainSoftness)
                stamps(canvas, part.dabs, part.argb, part.opacity, multiply, grain)
            }

            is RenderPart.Mask -> {}

            is RenderPart.Text -> {
                canvas.withMatrix(viewMatrix) { text(this, part) }
            }

            is RenderPart.Image -> {
                canvas.withMatrix(viewMatrix) { image(this, part) }
            }
        }
    }

    private fun outline(
        canvas: Canvas,
        points: FloatArray,
        contourEnds: IntArray,
        argb: Int,
        opacity: Float,
        evenOdd: Boolean,
        multiply: Boolean,
        cached: RenderPart.Outline?,
    ) {
        val target =
            if (cached == null) {
                path.also { buildPath(it, points, contourEnds, evenOdd) }
            } else {
                cached.nativePath as? Path ?: Path().also {
                    cached.nativePath = it
                    buildPath(it, points, contourEnds, evenOdd)
                }
            }
        fillPaint.color = argb
        fillPaint.alpha = alpha255(Color.alpha(argb) / MAX_ALPHA * opacity)
        fillPaint.blendMode = if (multiply) BlendMode.MULTIPLY else null
        canvas.withMatrix(viewMatrix) { drawPath(target, fillPaint) }
    }

    /**
     * One stamp stroke: coverage computed per pixel by [StampRaster] (core's per-dab falloff, grain and minimum
     * fade, union rule) at screen resolution, drawn as an alpha mask tinted with the stroke color at the stroke
     * opacity. Only the part of the stroke under the canvas clip is rasterized.
     */
    private fun stamps(
        canvas: Canvas,
        d: FloatArray,
        argb: Int,
        opacity: Float,
        multiply: Boolean,
        grain: GrainStyle,
    ) {
        val alpha = alpha255(Color.alpha(argb) / MAX_ALPHA * opacity)
        if (alpha == 0) return
        val clip = canvas.clipBounds.let { PixelRect(it.left, it.top, it.right, it.bottom) }
        val raster = RasterView(view.scale, -view.tx / view.scale, -view.ty / view.scale)
        val bitmap = stampBuffer.rasterize(d, if (grain.scale > 0f) grain else null, clip, raster) ?: return
        stampPaint.color = (argb and RGB_MASK) or (alpha shl ALPHA_SHIFT)
        stampPaint.blendMode = if (multiply) BlendMode.MULTIPLY else null
        canvas.drawBitmap(bitmap, stampBuffer.left.toFloat(), stampBuffer.top.toFloat(), stampPaint)
    }

    private fun mask(
        canvas: Canvas,
        mask: RenderPart.Mask,
    ) {
        val p = mask.points
        if (p.size < 2) return
        maskPaint.strokeWidth = mask.width
        if (p.size == 2) {
            fillPaint.blendMode = BlendMode.DST_OUT
            fillPaint.color = Color.BLACK
            canvas.drawCircle(p[0], p[1], mask.width / 2f, fillPaint)
            fillPaint.blendMode = null
            return
        }
        path.rewind()
        path.moveTo(p[0], p[1])
        var i = 2
        while (i + 1 < p.size) {
            path.lineTo(p[i], p[i + 1])
            i += 2
        }
        canvas.drawPath(path, maskPaint)
    }

    private fun text(
        canvas: Canvas,
        t: RenderPart.Text,
    ) {
        if (t.text.isEmpty() || t.boxWidth <= 0f) return
        corners(matrix, t.corners, t.boxWidth, t.boxHeight)
        textPaint.color = t.argb
        textPaint.alpha = alpha255(Color.alpha(t.argb) / MAX_ALPHA * t.opacity)
        textPaint.textSize = t.fontSize
        val layout =
            StaticLayout.Builder
                .obtain(t.text, 0, t.text.length, textPaint, t.boxWidth.toInt().coerceAtLeast(1))
                .setAlignment(Layout.Alignment.ALIGN_NORMAL)
                .build()
        canvas.withMatrix(matrix) { layout.draw(this) }
    }

    private fun image(
        canvas: Canvas,
        image: RenderPart.Image,
    ) {
        val bitmap = images()?.bitmap(image.asset) ?: return
        corners(matrix, image.corners, bitmap.width.toFloat(), bitmap.height.toFloat())
        canvas.drawBitmap(bitmap, matrix, imagePaint)
    }

    /** Maps the box `[0, w] × [0, h]` onto the transformed corners (tl, tr, br, bl) via corners 1, 2 and 4. */
    @Suppress("MagicNumber") // indexes into the 4-corner array and the 3-point poly
    private fun corners(
        into: Matrix,
        c: FloatArray,
        w: Float,
        h: Float,
    ) {
        src[0] = 0f
        src[1] = 0f
        src[2] = w
        src[3] = 0f
        src[4] = 0f
        src[5] = h
        dst[0] = c[0]
        dst[1] = c[1]
        dst[2] = c[2]
        dst[3] = c[3]
        dst[4] = c[6]
        dst[5] = c[7]
        into.setPolyToPoly(src, 0, dst, 0, TRIPLE)
    }

    private companion object {
        const val RGB_MASK = 0x00FFFFFF
        const val ALPHA_SHIFT = 24
        const val FULL_ALPHA = 255
        const val MAX_ALPHA = 255f
        const val LAYER_PAD_PX = 2f
        const val POLY = 6
        const val TRIPLE = 3
    }
}

private fun buildPath(
    into: Path,
    points: FloatArray,
    contourEnds: IntArray,
    evenOdd: Boolean,
) {
    into.rewind()
    into.fillType = if (evenOdd) Path.FillType.EVEN_ODD else Path.FillType.WINDING
    val count = points.size / 2
    var start = 0
    for (rawEnd in contourEnds) {
        val end = rawEnd.coerceAtMost(count)
        if (end - start > 2) {
            into.moveTo(points[2 * start], points[2 * start + 1])
            for (p in start + 1 until end) into.lineTo(points[2 * p], points[2 * p + 1])
            into.close()
        }
        start = end
    }
}
