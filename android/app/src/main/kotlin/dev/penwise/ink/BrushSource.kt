package dev.penwise.ink

/** Brush used for the next stroke; read on the UI thread at pen-down. */
class BrushSpec(
    val presetIndex: Int,
    val argb: Int,
    val sizeScale: Float,
)

fun interface BrushSource {
    fun current(): BrushSpec
}
