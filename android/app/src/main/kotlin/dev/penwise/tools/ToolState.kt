package dev.penwise.tools

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import dev.penwise.core.InkPresets
import dev.penwise.ink.BrushSource
import dev.penwise.ink.BrushSpec

/** Selected brush, color and size; observed by the toolbar, read by the canvas at pen-down. */
class ToolState : BrushSource {
    var presetIndex by mutableIntStateOf(0)
        private set

    /** Swatch color (RGB only, alpha comes from the preset default); null = preset default color. */
    var swatchRgb by mutableStateOf<Int?>(null)
        private set

    var sizeScale by mutableFloatStateOf(1f)
        private set

    fun selectPreset(index: Int) {
        if (index in InkPresets.names.indices) presetIndex = index
    }

    fun selectSwatch(rgb: Int?) {
        swatchRgb = rgb
    }

    fun changeSize(scale: Float) {
        sizeScale = scale.coerceIn(SIZE_MIN, SIZE_MAX)
    }

    fun presetArgb(index: Int = presetIndex): Int = InkPresets.defaultArgb.getOrElse(index) { BLACK }

    /** Effective stroke color: the swatch RGB keeps the preset's alpha (e.g. a translucent highlighter). */
    fun argb(): Int {
        val preset = presetArgb()
        return swatchRgb?.let { (preset and ALPHA_MASK) or (it and RGB_MASK) } ?: preset
    }

    override fun current(): BrushSpec = BrushSpec(presetIndex, argb(), sizeScale)

    companion object {
        const val SIZE_MIN = 0.5f
        const val SIZE_MAX = 3f
        private const val BLACK = -0x1000000
        private const val ALPHA_MASK = -0x1000000
        private const val RGB_MASK = 0x00FFFFFF
    }
}
