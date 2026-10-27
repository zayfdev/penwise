// Offsets into the flat float layout of `crates/penwise-ffi/src/render_floats.rs`: the layout is the contract.
@file:Suppress("MagicNumber")

package dev.penwise.ink

import kotlin.math.max
import kotlin.math.min

/** Axis-aligned page-space box; empty while `left > right`. */
internal class Bounds(
    var left: Float = Float.POSITIVE_INFINITY,
    var top: Float = Float.POSITIVE_INFINITY,
    var right: Float = Float.NEGATIVE_INFINITY,
    var bottom: Float = Float.NEGATIVE_INFINITY,
) {
    val isEmpty: Boolean get() = left > right || top > bottom

    fun add(
        x: Float,
        y: Float,
    ) {
        left = min(left, x)
        top = min(top, y)
        right = max(right, x)
        bottom = max(bottom, y)
    }

    fun add(other: Bounds) {
        if (other.isEmpty) return
        add(other.left, other.top)
        add(other.right, other.bottom)
    }

    fun intersects(other: Bounds): Boolean =
        !isEmpty && !other.isEmpty &&
            left <= other.right && right >= other.left && top <= other.bottom && bottom >= other.top

    fun copy(): Bounds = Bounds(left, top, right, bottom)

    fun inset(by: Float) {
        if (isEmpty) return
        left -= by
        top -= by
        right += by
        bottom += by
    }

    fun clear() {
        left = Float.POSITIVE_INFINITY
        top = Float.POSITIVE_INFINITY
        right = Float.NEGATIVE_INFINITY
        bottom = Float.NEGATIVE_INFINITY
    }
}

/** One drawable piece of an item (layout: `crates/penwise-ffi/src/render_floats.rs`). */
internal sealed interface RenderPart {
    /** Filled contours (`evenOdd` for fills, nonzero for ink). Points are x, y pairs, `contourEnds` in points. */
    class Outline(
        val argb: Int,
        val opacity: Float,
        val evenOdd: Boolean,
        val contourEnds: IntArray,
        val points: FloatArray,
        /** A shape's interior: core fills it with the Normal blend even when the outline multiplies. */
        val normalBlend: Boolean = false,
    ) : RenderPart {
        /** The painter's tessellated `android.graphics.Path`, built on first draw and reused until the item changes. */
        var nativePath: Any? = null
    }

    /** Dabs (9 floats each: x y rx ry angle opacity hardness grainStrength grainLevel) accumulated in one layer. */
    class Stamps(
        val argb: Int,
        val opacity: Float,
        val grainScale: Float,
        val grainStrength: Float,
        val grainSoftness: Float,
        val dabs: FloatArray,
    ) : RenderPart

    /** Destination-out capsule chain on the item alone (round caps and joins). */
    class Mask(
        val width: Float,
        val points: FloatArray,
    ) : RenderPart

    class Text(
        val corners: FloatArray,
        val argb: Int,
        val opacity: Float,
        val fontSize: Float,
        val boxWidth: Float,
        val boxHeight: Float,
        val text: String,
    ) : RenderPart

    class Image(
        val corners: FloatArray,
        val asset: String,
    ) : RenderPart
}

/** A decoded `sceneItemFloats` array with its page-space bounds. */
internal class ItemRender(
    val multiply: Boolean,
    val parts: List<RenderPart>,
    val bounds: Bounds,
) {
    val masks: List<RenderPart.Mask> get() = parts.filterIsInstance<RenderPart.Mask>()

    companion object {
        private const val KIND_OUTLINE = 0
        private const val KIND_STAMPS = 1
        private const val KIND_EVEN_ODD = 2
        private const val KIND_MASK = 3
        private const val KIND_TEXT = 4
        private const val KIND_IMAGE = 5
        private const val KIND_SHAPE_FILL = 6
        private const val PAD_PX = 2f
        private const val DAB = 9
        private const val HASH_BYTES = 32

        /** Null for an empty array (the item is not drawable). [text] is only read for text items. */
        fun parse(
            floats: FloatArray,
            text: () -> String,
        ): ItemRender? {
            if (floats.size < 2) return null
            val parts = ArrayList<RenderPart>()
            var at = 2
            repeat(floats[1].toInt()) {
                val kind = floats[at].toInt()
                val len = floats[at + 1].toInt()
                parts += part(kind, floats, at + 2, text)
                at += 2 + len
            }
            val bounds = Bounds()
            parts.forEach { addBounds(bounds, it) }
            bounds.inset(PAD_PX)
            return ItemRender(floats[0] != 0f, parts, bounds)
        }

        private fun part(
            kind: Int,
            f: FloatArray,
            o: Int,
            text: () -> String,
        ): RenderPart =
            when (kind) {
                KIND_OUTLINE, KIND_EVEN_ODD, KIND_SHAPE_FILL -> {
                    outline(f, o, kind == KIND_EVEN_ODD, kind == KIND_SHAPE_FILL)
                }

                KIND_STAMPS -> {
                    stamps(f, o)
                }

                KIND_MASK -> {
                    val n = f[o + 1].toInt()
                    RenderPart.Mask(f[o], f.copyOfRange(o + 2, o + 2 + 2 * n))
                }

                KIND_TEXT -> {
                    RenderPart.Text(
                        corners = f.copyOfRange(o, o + 8),
                        argb = argb(f, o + 8),
                        opacity = f[o + 12],
                        fontSize = f[o + 13],
                        boxWidth = f[o + 14],
                        boxHeight = f[o + 15],
                        text = text(),
                    )
                }

                KIND_IMAGE -> {
                    RenderPart.Image(
                        f.copyOfRange(o, o + 8),
                        buildString { for (i in 0 until HASH_BYTES) append("%02x".format(f[o + 8 + i].toInt())) },
                    )
                }

                else -> {
                    error("unknown render part $kind")
                }
            }

        private fun argb(
            f: FloatArray,
            o: Int,
        ): Int = (f[o + 3].toInt() shl 24) or (f[o].toInt() shl 16) or (f[o + 1].toInt() shl 8) or f[o + 2].toInt()

        private fun outline(
            f: FloatArray,
            o: Int,
            evenOdd: Boolean,
            normalBlend: Boolean,
        ): RenderPart.Outline {
            val contours = f[o + 5].toInt()
            val ends = IntArray(contours) { f[o + 6 + it].toInt() }
            val pointCount = ends.lastOrNull() ?: 0
            val start = o + 6 + contours
            val points = f.copyOfRange(start, start + 2 * pointCount)
            return RenderPart.Outline(argb(f, o), f[o + 4], evenOdd, ends, points, normalBlend)
        }

        private fun stamps(
            f: FloatArray,
            o: Int,
        ): RenderPart.Stamps {
            val n = f[o + 8].toInt()
            val dabs = f.copyOfRange(o + 9, o + 9 + n * DAB)
            return RenderPart.Stamps(argb(f, o), f[o + 4], f[o + 5], f[o + 6], f[o + 7], dabs)
        }

        private fun addBounds(
            out: Bounds,
            part: RenderPart,
        ) {
            when (part) {
                is RenderPart.Outline -> {
                    addPoints(out, part.points, 0f)
                }

                is RenderPart.Stamps -> {
                    var i = 0
                    while (i + DAB <= part.dabs.size) {
                        val r = max(part.dabs[i + 2], part.dabs[i + 3])
                        out.add(part.dabs[i] - r, part.dabs[i + 1] - r)
                        out.add(part.dabs[i] + r, part.dabs[i + 1] + r)
                        i += DAB
                    }
                }

                is RenderPart.Mask -> {}

                is RenderPart.Text -> {
                    addPoints(out, part.corners, 0f)
                }

                is RenderPart.Image -> {
                    addPoints(out, part.corners, 0f)
                }
            }
        }

        private fun addPoints(
            out: Bounds,
            points: FloatArray,
            pad: Float,
        ) {
            var i = 0
            while (i + 1 < points.size) {
                out.add(points[i] - pad, points[i + 1] - pad)
                out.add(points[i] + pad, points[i + 1] + pad)
                i += 2
            }
        }
    }
}
