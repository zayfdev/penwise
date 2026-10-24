package dev.penwise.ink

/** Point-to-polyline distance test used by the stroke eraser; samples are already in view px. */
internal object EraseHit {
    private const val STRIDE = SavedStrokeCodec.STRIDE
    private const val PAIR = 2
    private const val MIN_X = 0
    private const val MIN_Y = 1
    private const val MAX_X = 2
    private const val MAX_Y = 3

    /**
     * True when any of the first [count] points of [points] (x, y interleaved) lies within [radius] of the
     * stroke's centreline. A stroke with one sample is a dot.
     */
    fun hits(
        stroke: SavedStroke,
        points: FloatArray,
        count: Int,
        radius: Float,
    ): Boolean {
        if (stroke.count == 0 || count == 0) return false
        val box = bounds(stroke) ?: return false
        var i = 0
        while (i < count) {
            val px = points[i * PAIR]
            val py = points[i * PAIR + 1]
            if (inBox(box, px, py, radius) && within(stroke, px, py, radius)) return true
            i++
        }
        return false
    }

    private fun inBox(
        box: FloatArray,
        px: Float,
        py: Float,
        radius: Float,
    ): Boolean =
        px >= box[MIN_X] - radius &&
            px <= box[MAX_X] + radius &&
            py >= box[MIN_Y] - radius &&
            py <= box[MAX_Y] + radius

    /** `[minX, minY, maxX, maxY]` of the stroke's samples, or null when they are not finite. */
    private fun bounds(stroke: SavedStroke): FloatArray? {
        var minX = Float.MAX_VALUE
        var minY = Float.MAX_VALUE
        var maxX = -Float.MAX_VALUE
        var maxY = -Float.MAX_VALUE
        for (s in 0 until stroke.count) {
            val x = stroke.samples[s * STRIDE]
            val y = stroke.samples[s * STRIDE + 1]
            if (!x.isFinite() || !y.isFinite()) return null
            if (x < minX) minX = x
            if (y < minY) minY = y
            if (x > maxX) maxX = x
            if (y > maxY) maxY = y
        }
        return floatArrayOf(minX, minY, maxX, maxY)
    }

    private fun within(
        stroke: SavedStroke,
        px: Float,
        py: Float,
        radius: Float,
    ): Boolean {
        if (stroke.count == 1) {
            return distance(stroke.samples[0], stroke.samples[1], px, py) <= radius
        }
        for (s in 0 until stroke.count - 1) {
            val ax = stroke.samples[s * STRIDE]
            val ay = stroke.samples[s * STRIDE + 1]
            val bx = stroke.samples[(s + 1) * STRIDE]
            val by = stroke.samples[(s + 1) * STRIDE + 1]
            if (segmentDistance(ax, ay, bx, by, px, py) <= radius) return true
        }
        return false
    }

    private fun distance(
        ax: Float,
        ay: Float,
        bx: Float,
        by: Float,
    ): Float {
        val dx = bx - ax
        val dy = by - ay
        return kotlin.math.sqrt(dx * dx + dy * dy)
    }

    private fun segmentDistance(
        ax: Float,
        ay: Float,
        bx: Float,
        by: Float,
        px: Float,
        py: Float,
    ): Float {
        val dx = bx - ax
        val dy = by - ay
        val lengthSquared = dx * dx + dy * dy
        if (lengthSquared <= 0f) return distance(ax, ay, px, py)
        var t = ((px - ax) * dx + (py - ay) * dy) / lengthSquared
        if (t < 0f) t = 0f
        if (t > 1f) t = 1f
        return distance(ax + t * dx, ay + t * dy, px, py)
    }
}
