// `ShapeKind::code` values are frozen in core; the numbers are the contract.
@file:Suppress("MagicNumber")

package dev.penwise.ink

/**
 * Shape tools: drag from corner to corner (or end to end) instead of drawing freehand. [code] is the core
 * `ShapeKind::code`, [param] its one numeric parameter (an arrow's head: 0 = end only).
 */
enum class ShapeMode(
    val code: Int,
    val param: Float = 0f,
) {
    Line(3),
    Rect(0),
    Ellipse(2),
    Arrow(4),
}

/** Look of a new shape; the geometry comes from the drag. A fill colour is optional. */
class ShapeStyle(
    val kind: ShapeMode,
    val preset: Int,
    val argb: Int,
    val scale: Float,
    val constrain: Boolean = false,
    val fillArgb: Int = 0,
    val hasFill: Boolean = false,
)
