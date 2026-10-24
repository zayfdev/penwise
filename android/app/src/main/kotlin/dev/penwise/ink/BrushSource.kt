package dev.penwise.ink

/** Brush used for the next stroke; read on the UI thread at pen-down. */
class BrushSpec(
    val presetIndex: Int,
    val argb: Int,
    val sizeScale: Float,
)

interface BrushSource {
    fun current(): BrushSpec

    /** True when the pen erases whole strokes instead of drawing. */
    val eraserMode: Boolean get() = false

    /** Eraser contact radius in view px. */
    val eraserRadiusPx: Float get() = 16f

    /** Shape tool: drag to create instead of drawing freehand; null = freehand. */
    val shapeMode: ShapeMode? get() = null
}
