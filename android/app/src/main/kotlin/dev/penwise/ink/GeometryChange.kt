package dev.penwise.ink

import dev.penwise.core.InkGeometry
import kotlin.math.max
import kotlin.math.min

/**
 * Where two snapshots of the same growing stroke differ. The core only ever recomputes the tentative tail, so
 * consecutive frames share a long run of identical vertices (outline) or dabs (stamps) at the start and, for an
 * outline, at the end of the polygon (the stable right side and the start cap). The front buffer clears and redraws
 * only the box around what is left in between: redrawing the whole stroke every frame makes the clear visible
 * (the single-buffered layer is scanned out while it is half repainted), and the cost grows with every hold.
 */
internal object GeometryChange {
    private const val PAD_PAGE_PX = 2f

    /** Slots of the box array. */
    const val LEFT = 0
    const val TOP = 1
    const val RIGHT = 2
    const val BOTTOM = 3

    /**
     * Writes the padded page-space box (left, top, right, bottom) of everything that differs between [old] and
     * [new] into [box]; false when nothing does. Geometry of different kinds, or with several contours, counts as
     * wholly changed.
     */
    fun between(
        old: InkGeometry,
        new: InkGeometry,
        box: FloatArray,
    ): Boolean {
        val sameKind = old.isStamps == new.isStamps
        val comparable = sameKind && (new.isStamps || (old.contourEnds.size <= 1 && new.contourEnds.size <= 1))
        if (!comparable) return everything(old, new, box)
        return if (new.isStamps) stamps(old.dabs, new.dabs, box) else outline(old.outline, new.outline, box)
    }

    /**
     * Writes the padded page-space box of what differs between two polylines (x,y pairs; lasso loop) into [box];
     * false when they are equal. A polyline that only grew differs from the last old vertex on.
     */
    fun polyline(
        old: FloatArray,
        new: FloatArray,
        box: FloatArray,
    ): Boolean = outline(old, new, box)

    private fun outline(
        old: FloatArray,
        new: FloatArray,
        box: FloatArray,
    ): Boolean {
        val na = old.size / 2
        val nb = new.size / 2
        val shared = min(na, nb)
        var prefix = 0
        while (prefix < shared && same(old, new, prefix, prefix)) prefix++
        if (prefix == na && prefix == nb) return false
        var suffix = 0
        while (suffix < shared - prefix && same(old, new, na - 1 - suffix, nb - 1 - suffix)) suffix++
        reset(box)
        // The vertices next to the shared runs carry the edges that did change.
        include(box, old, max(prefix - 1, 0), min(na - suffix + 1, na))
        include(box, new, max(prefix - 1, 0), min(nb - suffix + 1, nb))
        pad(box)
        return true
    }

    private fun stamps(
        old: FloatArray,
        new: FloatArray,
        box: FloatArray,
    ): Boolean {
        val na = old.size / Dab.STRIDE
        val nb = new.size / Dab.STRIDE
        var prefix = 0
        while (prefix < min(na, nb) && sameDab(old, new, prefix)) prefix++
        if (prefix == na && prefix == nb) return false
        reset(box)
        for (i in prefix until na) includeDab(box, old, i)
        for (i in prefix until nb) includeDab(box, new, i)
        pad(box)
        return true
    }

    private fun everything(
        old: InkGeometry,
        new: InkGeometry,
        box: FloatArray,
    ): Boolean {
        reset(box)
        for (g in arrayOf(old, new)) {
            if (g.isStamps) {
                for (i in 0 until g.dabs.size / Dab.STRIDE) includeDab(box, g.dabs, i)
            } else {
                include(box, g.outline, 0, g.outline.size / 2)
            }
        }
        pad(box)
        return box[LEFT] < box[RIGHT]
    }

    private fun same(
        a: FloatArray,
        b: FloatArray,
        i: Int,
        j: Int,
    ) = a[2 * i] == b[2 * j] && a[2 * i + 1] == b[2 * j + 1]

    private fun sameDab(
        a: FloatArray,
        b: FloatArray,
        i: Int,
    ): Boolean {
        for (k in 0 until Dab.STRIDE) if (a[i * Dab.STRIDE + k] != b[i * Dab.STRIDE + k]) return false
        return true
    }

    private fun reset(box: FloatArray) {
        box[LEFT] = Float.POSITIVE_INFINITY
        box[TOP] = Float.POSITIVE_INFINITY
        box[RIGHT] = Float.NEGATIVE_INFINITY
        box[BOTTOM] = Float.NEGATIVE_INFINITY
    }

    private fun include(
        box: FloatArray,
        points: FloatArray,
        from: Int,
        until: Int,
    ) {
        for (i in from until until) {
            box[LEFT] = min(box[LEFT], points[2 * i])
            box[TOP] = min(box[TOP], points[2 * i + 1])
            box[RIGHT] = max(box[RIGHT], points[2 * i])
            box[BOTTOM] = max(box[BOTTOM], points[2 * i + 1])
        }
    }

    private fun includeDab(
        box: FloatArray,
        dabs: FloatArray,
        i: Int,
    ) {
        val o = i * Dab.STRIDE
        val r = max(dabs[o + Dab.RADIUS_X], dabs[o + Dab.RADIUS_Y])
        box[LEFT] = min(box[LEFT], dabs[o + Dab.X] - r)
        box[TOP] = min(box[TOP], dabs[o + Dab.Y] - r)
        box[RIGHT] = max(box[RIGHT], dabs[o + Dab.X] + r)
        box[BOTTOM] = max(box[BOTTOM], dabs[o + Dab.Y] + r)
    }

    private fun pad(box: FloatArray) {
        box[LEFT] -= PAD_PAGE_PX
        box[TOP] -= PAD_PAGE_PX
        box[RIGHT] += PAD_PAGE_PX
        box[BOTTOM] += PAD_PAGE_PX
    }
}
