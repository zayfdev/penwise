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

/** Draws render data on a Canvas. Not thread-safe: one instance per render layer. */
internal class GeometryPainter(
    private val grain: GrainMasks,
    private val images: () -> PageImages? = { null },
) {
    private val path = Path()
    private val bounds = RectF()
    private val shaders = DabShaders()
    private val fillPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply { style = Paint.Style.FILL }
    private val dabPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = Color.BLACK }
    private val layerPaint = Paint()
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
    private val src = FloatArray(POLY)
    private val dst = FloatArray(POLY)

    /** The in-progress stroke straight from the ink engine (front buffer). */
    fun draw(
        canvas: Canvas,
        geometry: InkGeometry,
    ) {
        if (geometry.isStamps) {
            geometry.boundsInto(bounds)
            val d = geometry.dabs
            val strength = if (d.size >= Dab.STRIDE) d[Dab.GRAIN_STRENGTH] else 0f
            val level = if (d.size >= Dab.STRIDE) d[Dab.GRAIN_LEVEL] else 0f
            val grainParams = Grain(1f, strength, level)
            stamps(canvas, geometry.dabs, geometry.argb, geometry.opacity, geometry.multiply, grainParams)
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
        layerPaint.alpha = FULL_ALPHA
        layerPaint.blendMode = if (item.multiply) BlendMode.MULTIPLY else null
        val save = canvas.saveLayer(bounds, layerPaint)
        item.parts.forEach { if (it !is RenderPart.Mask) part(canvas, it, false) }
        masks.forEach { mask(canvas, it) }
        canvas.restoreToCount(save)
    }

    private fun part(
        canvas: Canvas,
        part: RenderPart,
        multiply: Boolean,
    ) {
        when (part) {
            is RenderPart.Outline -> {
                outline(canvas, part.points, part.contourEnds, part.argb, part.opacity, part.evenOdd, multiply, part)
            }

            is RenderPart.Stamps -> {
                setBounds(part.dabs)
                val first = part.dabs
                val level = if (first.size >= Dab.STRIDE) first[Dab.GRAIN_LEVEL] else 0f
                val grainParams = Grain(part.grainScale, part.grainStrength, level, part.grainSoftness)
                stamps(canvas, part.dabs, part.argb, part.opacity, multiply, grainParams)
            }

            is RenderPart.Mask -> {}

            is RenderPart.Text -> {
                text(canvas, part)
            }

            is RenderPart.Image -> {
                image(canvas, part)
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
        canvas.drawPath(target, fillPaint)
    }

    /** Dabs accumulate inside one layer, so overlaps never exceed the stroke opacity applied at restore. */
    private fun stamps(
        canvas: Canvas,
        d: FloatArray,
        argb: Int,
        opacity: Float,
        multiply: Boolean,
        grainParams: Grain,
    ) {
        if (bounds.isEmpty) return
        layerPaint.alpha = alpha255(Color.alpha(argb) / MAX_ALPHA * opacity)
        layerPaint.blendMode = if (multiply) BlendMode.MULTIPLY else null
        val save = canvas.saveLayer(bounds, layerPaint)
        var i = 0
        while (i + Dab.STRIDE <= d.size) {
            dab(canvas, d, i)
            i += Dab.STRIDE
        }
        grain.paint(grainParams.strength, grainParams.level, grainParams.softness, grainParams.scale)?.let {
            canvas.drawRect(bounds, it)
        }
        canvas.drawColor(argb or OPAQUE, BlendMode.SRC_IN)
        canvas.restoreToCount(save)
    }

    private fun dab(
        canvas: Canvas,
        d: FloatArray,
        o: Int,
    ) {
        val rx = d[o + Dab.RADIUS_X]
        val ry = d[o + Dab.RADIUS_Y]
        if (rx <= 0f || ry <= 0f) return
        dabPaint.shader = shaders.forHardness(d[o + Dab.HARDNESS])
        dabPaint.alpha = alpha255(d[o + Dab.OPACITY])
        canvas.withTranslation(d[o + Dab.X], d[o + Dab.Y]) {
            rotate(Math.toDegrees(d[o + Dab.ANGLE].toDouble()).toFloat())
            scale(rx, ry)
            drawCircle(0f, 0f, 1f, dabPaint)
        }
    }

    private fun setBounds(dabs: FloatArray) {
        val b = Bounds()
        var i = 0
        while (i + Dab.STRIDE <= dabs.size) {
            val r = maxOf(dabs[i + Dab.RADIUS_X], dabs[i + Dab.RADIUS_Y])
            b.add(dabs[i + Dab.X] - r, dabs[i + Dab.Y] - r)
            b.add(dabs[i + Dab.X] + r, dabs[i + Dab.Y] + r)
            i += Dab.STRIDE
        }
        if (b.isEmpty) bounds.setEmpty() else bounds.set(b.left, b.top, b.right, b.bottom)
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
        const val OPAQUE = -0x1000000
        const val FULL_ALPHA = 255
        const val MAX_ALPHA = 255f
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

/** Paper-grain parameters of one stamp stroke. */
internal class Grain(
    val scale: Float,
    val strength: Float,
    val level: Float,
    val softness: Float = DEFAULT_SOFTNESS,
) {
    companion object {
        const val DEFAULT_SOFTNESS = 0.2f
    }
}
