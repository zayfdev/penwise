package dev.penwise.ink

import kotlin.math.ceil
import kotlin.math.cos
import kotlin.math.floor
import kotlin.math.max
import kotlin.math.min
import kotlin.math.sin

/** Page → pixel mapping of a render (core `Viewport`): `px = (page - origin) * scale`. The tablet draws 1:1. */
internal class RasterView(
    val scale: Float = 1f,
    val originX: Float = 0f,
    val originY: Float = 0f,
)

/** A stroke's paper grain (`scale == 0` means none); level and strength travel with each dab. */
internal class GrainStyle(
    val scale: Float,
    val softness: Float,
)

/** Pixel rect `[left, right) x [top, bottom)`. */
internal class PixelRect(
    val left: Int,
    val top: Int,
    val right: Int,
    val bottom: Int,
) {
    val width: Int get() = right - left
    val height: Int get() = bottom - top

    operator fun contains(p: Pair<Int, Int>): Boolean = p.first in left until right && p.second in top until bottom
}

/** Paper height at a page position. */
internal fun interface HeightSource {
    fun height(
        x: Float,
        y: Float,
    ): Float
}

/** A dab in pixel space with the minimum fade applied (core `PixelDab`). */
internal class PixelDab(
    val cx: Float,
    val cy: Float,
    val invRx2: Float,
    val invRy2: Float,
    /** Circumscribing radius (bbox half-extent). */
    val extent: Float,
    val sin: Float,
    val cos: Float,
    val hardness: Float,
    val opacity: Float,
    val grainStrength: Float,
    val grainLevel: Float,
)

/** The scalar formulas of core's stamp renderer: falloff, minimum fade and grain coverage. */
internal object DabMath {
    private const val MIN_FADE_HARDNESS = 0.1f
    private const val TWO = 2f
    private const val THREE = 3f

    /** Core `dab_falloff`: MyPaint two-segment falloff at `rr = (dist/r)^2`. */
    fun falloff(
        rr: Float,
        hardness: Float,
    ): Float {
        if (rr.isNaN() || rr > 1f) return 0f
        if (hardness.isNaN() || hardness >= 1f) return 1f
        if (hardness <= 0f) return 0f
        val r = max(rr, 0f)
        val opa = if (r <= hardness) 1f + r * (1f - 1f / hardness) else hardness / (1f - hardness) * (1f - r)
        return opa.coerceIn(0f, 1f)
    }

    /** Core `min_fade`: `(radius', hardness', faded)` for a pixel radius (keeps the optical radius, >= 1 px fade). */
    fun minFade(
        radius: Float,
        hardness: Float,
    ): Triple<Float, Float, Boolean> {
        val needsFade = radius > 0f && radius.isFinite() && radius * (1f - hardness) < 1f
        if (!needsFade) return Triple(radius, hardness, false)
        val rOpt = radius - (1f - hardness) * radius / 2f
        val h = max((rOpt - 0.5f) / (rOpt + 0.5f), MIN_FADE_HARDNESS)
        return Triple(1f / (1f - h), h, true)
    }

    /** Core `grain_coverage`: `lerp(1, smoothstep(1 - level - w, 1 - level + w, height), strength)`. */
    fun grainCoverage(
        height: Float,
        level: Float,
        softness: Float,
        strength: Float,
    ): Float {
        val edge = 1f - level
        return 1f + (smoothstep(edge - softness, edge + softness, height) - 1f) * strength
    }

    private fun smoothstep(
        e0: Float,
        e1: Float,
        x: Float,
    ): Float {
        if (e1 <= e0) return if (x >= e0) 1f else 0f
        val t = ((x - e0) / (e1 - e0)).coerceIn(0f, 1f)
        return t * t * (THREE - TWO * t)
    }
}

/**
 * Stamp stroke coverage, the tablet's copy of core `raster::stamps` (the formula is documented in that module).
 * Pure math, no Android types: `StampRasterTest` replays `crates/penwise-core/tests/goldens/stamp_coverage.txt`
 * through it and compares with core's coverage. [GeometryPainter] turns the coverage into pixels.
 */
internal object StampRaster {
    private const val MIN_HARDNESS = 1e-3f
    private const val CENTRE = 0.5f

    /** Core `PixelDab::new` for the dab at `d[o]` (`Dab.STRIDE` floats); null for invisible or malformed dabs. */
    fun pixelDab(
        d: FloatArray,
        o: Int,
        view: RasterView,
    ): PixelDab? {
        val opacity = d[o + Dab.OPACITY].coerceIn(0f, 1f)
        val rxPage = d[o + Dab.RADIUS_X]
        val ryPage = d[o + Dab.RADIUS_Y]
        val finite =
            d[o + Dab.X].isFinite() &&
                d[o + Dab.Y].isFinite() &&
                d[o + Dab.ANGLE].isFinite() &&
                rxPage.isFinite() &&
                ryPage.isFinite()
        if (!(rxPage > 0f && ryPage > 0f && opacity > 0f && finite)) return null
        val cx = (d[o + Dab.X] - view.originX) * view.scale
        val cy = (d[o + Dab.Y] - view.originY) * view.scale
        val rx = rxPage * view.scale
        val ry = ryPage * view.scale
        val h = max(d[o + Dab.HARDNESS].coerceIn(0f, 1f), MIN_HARDNESS)
        // Fade along the minor axis; the major axis keeps its optical radius but never ends up shorter than the
        // faded minor one (core explains why).
        val (rMinor, fadedH, faded) = DabMath.minFade(min(rx, ry), h)
        val major = max(rx, ry)
        val rMajor = if (faded) max(major * (1f + h) / (1f + fadedH), rMinor) else major
        val (fx, fy) = if (rx < ry) rMinor to rMajor else rMajor to rMinor
        val extent = max(fx, fy)
        if (!(cx.isFinite() && cy.isFinite() && extent.isFinite())) return null
        val angle = d[o + Dab.ANGLE]
        return PixelDab(
            cx,
            cy,
            1f / (fx * fx),
            1f / (fy * fy),
            extent,
            sin(angle),
            cos(angle),
            fadedH,
            opacity,
            d[o + Dab.GRAIN_STRENGTH].coerceIn(0f, 1f),
            d[o + Dab.GRAIN_LEVEL],
        )
    }

    /** The visible pixel dabs of `d` (`Dab.STRIDE` floats each). */
    fun pixelDabs(
        d: FloatArray,
        view: RasterView,
    ): List<PixelDab> {
        val out = ArrayList<PixelDab>(d.size / Dab.STRIDE)
        var o = 0
        while (o + Dab.STRIDE <= d.size) {
            pixelDab(d, o, view)?.let { out += it }
            o += Dab.STRIDE
        }
        return out
    }

    /** Falloff of one dab at the centre of pixel `(col, row)`; 0 outside it. */
    fun falloffAt(
        pd: PixelDab,
        col: Int,
        row: Int,
    ): Float {
        val fx = col + CENTRE - pd.cx
        val fy = row + CENTRE - pd.cy
        val xr = fx * pd.cos + fy * pd.sin
        val yr = fy * pd.cos - fx * pd.sin
        return DabMath.falloff(xr * xr * pd.invRx2 + yr * yr * pd.invRy2, pd.hardness)
    }

    /** The dab's alpha `falloff * opacity * grain` given its falloff `f > 0` and the paper height under the pixel. */
    fun shade(
        pd: PixelDab,
        f: Float,
        height: Float,
        grain: GrainStyle?,
    ): Float {
        var a = f * pd.opacity
        if (grain != null && pd.grainStrength > 0f) {
            a *= DabMath.grainCoverage(height, pd.grainLevel, grain.softness, pd.grainStrength)
        }
        return a.coerceIn(0f, 1f)
    }

    /** Coverage `S` of pixel `(col, row)`: the union of the dabs in order (the reference for the parity tests). */
    fun coverageAt(
        dabs: List<PixelDab>,
        col: Int,
        row: Int,
        view: RasterView,
        grain: GrainStyle?,
        heights: HeightSource?,
    ): Float {
        var s = 0f
        for (pd in dabs) {
            val f = falloffAt(pd, col, row)
            if (f <= 0f) continue
            val grained = grain != null && pd.grainStrength > 0f
            val h = if (grained && heights != null) paperAt(heights, col, row, view) else 0f
            s += shade(pd, f, h, grain) * (1f - s)
        }
        return s
    }

    /** Pixel rect of the dabs clipped to [clip]; null when empty. */
    fun rect(
        dabs: List<PixelDab>,
        clip: PixelRect,
    ): PixelRect? {
        var x0 = Float.POSITIVE_INFINITY
        var y0 = Float.POSITIVE_INFINITY
        var x1 = Float.NEGATIVE_INFINITY
        var y1 = Float.NEGATIVE_INFINITY
        for (pd in dabs) {
            x0 = min(x0, pd.cx - pd.extent)
            y0 = min(y0, pd.cy - pd.extent)
            x1 = max(x1, pd.cx + pd.extent)
            y1 = max(y1, pd.cy + pd.extent)
        }
        val left = max(clip.left.toFloat(), floor(x0)).toInt()
        val top = max(clip.top.toFloat(), floor(y0)).toInt()
        val right = min(clip.right.toFloat(), ceil(x1)).toInt()
        val bottom = min(clip.bottom.toFloat(), ceil(y1)).toInt()
        return if (right > left && bottom > top) PixelRect(left, top, right, bottom) else null
    }

    /**
     * Accumulates [dabs] into [buffer] (`rect.width * rect.height` values over [rect]; the array may be longer) with
     * the union rule, dab by dab like core's `render_stamps`. Paper heights are read once per pixel, into [scratch]
     * (grown by [paperScratch] when too small; the array used is returned so the caller keeps it), so a grained
     * stroke does not allocate a buffer per call.
     */
    fun accumulate(
        buffer: FloatArray,
        rect: PixelRect,
        dabs: List<PixelDab>,
        view: RasterView,
        grain: GrainStyle?,
        heights: HeightSource?,
        scratch: FloatArray? = null,
    ): FloatArray? {
        val paper = if (grain != null && heights != null) paperScratch(scratch, rect.width * rect.height) else null
        for (pd in dabs) {
            val lx = max(rect.left, floor(pd.cx - pd.extent).toInt())
            val hx = min(rect.right, ceil(pd.cx + pd.extent).toInt())
            val ly = max(rect.top, floor(pd.cy - pd.extent).toInt())
            val hy = min(rect.bottom, ceil(pd.cy + pd.extent).toInt())
            for (row in ly until hy) stampRow(buffer, rect, pd, row, lx until hx, view, grain, heights, paper)
        }
        return paper ?: scratch
    }

    /** The paper-height cache for [n] pixels: [scratch] if big enough, else a new array of exactly [n]; NaN-filled. */
    fun paperScratch(
        scratch: FloatArray?,
        n: Int,
    ): FloatArray {
        val paper = if (scratch != null && scratch.size >= n) scratch else FloatArray(n)
        paper.fill(Float.NaN, 0, n)
        return paper
    }

    @Suppress("LongParameterList") // one row of one dab: all the context the pixels need
    private fun stampRow(
        buffer: FloatArray,
        rect: PixelRect,
        pd: PixelDab,
        row: Int,
        cols: IntRange,
        view: RasterView,
        grain: GrainStyle?,
        heights: HeightSource?,
        paper: FloatArray?,
    ) {
        for (col in cols) {
            val f = falloffAt(pd, col, row)
            if (f <= 0f) continue
            val i = (row - rect.top) * rect.width + (col - rect.left)
            var h = 0f
            if (paper != null && heights != null && pd.grainStrength > 0f) {
                if (paper[i].isNaN()) paper[i] = paperAt(heights, col, row, view)
                h = paper[i]
            }
            val s = buffer[i]
            buffer[i] = s + shade(pd, f, h, grain) * (1f - s)
        }
    }

    /** Paper height under the centre of pixel `(col, row)`: page position `p / scale + origin`. */
    private fun paperAt(
        heights: HeightSource,
        col: Int,
        row: Int,
        view: RasterView,
    ): Float = heights.height((col + CENTRE) / view.scale + view.originX, (row + CENTRE) / view.scale + view.originY)
}
