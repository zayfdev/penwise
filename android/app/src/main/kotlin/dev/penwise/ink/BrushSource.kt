package dev.penwise.ink

/** Brush used for the next stroke; read on the UI thread at pen-down. */
class BrushSpec(
    val presetIndex: Int,
    val argb: Int,
    val sizeScale: Float,
)

interface BrushSource {
    fun current(): BrushSpec

    /** True when the pen erases instead of drawing. */
    val eraserMode: Boolean get() = false

    /** Eraser contact radius in view px. */
    val eraserRadiusPx: Float get() = 16f

    /** True when the eraser cuts strokes at the sweep instead of removing them whole. */
    val preciseErase: Boolean get() = false

    /** Shape tool: drag to create instead of drawing freehand; null = freehand. */
    val shapeMode: ShapeMode? get() = null

    /** Lasso tool: the pen selects, and drags the selection. */
    val lassoMode: Boolean get() = false

    /** What the lasso selected; null when the source has no lasso tool. */
    val selection: Selection? get() = null
}
