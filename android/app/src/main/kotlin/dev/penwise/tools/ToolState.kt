package dev.penwise.tools

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import dev.penwise.core.InkPresets
import dev.penwise.ink.BrushSource
import dev.penwise.ink.BrushSpec
import dev.penwise.ink.Selection
import dev.penwise.ink.ShapeMode

/** Selected brush, color and size; observed by the toolbar, read by the canvas at pen-down. */
class ToolState internal constructor(
    /** The colours picked from the wheel lately; the app passes one that is saved. */
    internal val recents: RecentColours = RecentColours(),
) : BrushSource {
    var presetIndex by mutableIntStateOf(0)
        private set

    /** Swatch color (RGB only, alpha comes from the preset default); null = preset default color. */
    var swatchRgb by mutableStateOf<Int?>(null)
        private set

    var sizeScale by mutableFloatStateOf(1f)
        private set

    fun selectPreset(index: Int) {
        if (index !in InkPresets.names.indices) return
        presetIndex = index
        if (index != markerIndex) penIndex = index
    }

    /** The last pen picked that was not the marker: the Pen button goes back to it. */
    var penIndex by mutableIntStateOf(0)
        private set

    private val markerIndex: Int by lazy { InkPresets.names.indexOf(MARKER) }

    /** True while the marker brush is the selected one. */
    val markerSelected: Boolean get() = presetIndex == markerIndex

    /** True while the pen writes: no eraser, no lasso, no shape tool. */
    val drawing: Boolean get() = !eraserMode && !lassoMode && shapeMode == null

    /** Back to writing with the last pen. */
    fun selectPen() {
        selectPreset(penIndex)
        draw()
    }

    /** Writing with the marker brush. */
    fun selectMarker() {
        selectPreset(markerIndex)
        draw()
    }

    fun selectSwatch(rgb: Int?) {
        swatchRgb = rgb?.let { it and RGB_MASK }
    }

    /** Remembers the current wheel colour as a recent pick; called when a wheel gesture ends. */
    internal fun commitColour() {
        swatchRgb?.let(recents::remember)
    }

    /** Back to drawing: leaves the eraser, the shapes and the lasso (picking a pen means to write with it). */
    fun draw() {
        eraserMode = false
        lassoMode = false
        shapeMode = null
        selection.clear()
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

    /** True while the eraser tool is selected; the pen then erases instead of drawing. */
    override var eraserMode by mutableStateOf(false)
        private set

    /** Eraser radius derived from the size slider (0.5–3× maps to [ERASER_MIN_PX]–[ERASER_MAX_PX] px). */
    override val eraserRadiusPx: Float
        get() {
            val t = (sizeScale - SIZE_MIN) / (SIZE_MAX - SIZE_MIN)
            return ERASER_MIN_PX + t * (ERASER_MAX_PX - ERASER_MIN_PX)
        }

    /** Precise erase cuts strokes at the sweep; off, the eraser removes whole strokes. */
    override var preciseErase by mutableStateOf(true)
        private set

    fun togglePreciseErase() {
        preciseErase = !preciseErase
    }

    fun toggleEraser() {
        eraserMode = !eraserMode
        selection.clear()
        if (eraserMode) {
            shapeMode = null
            lassoMode = false
        }
    }

    /**
     * Strokes started on a frame follow the element under the pen (true, the default) or are pinned to the page
     * position (false). Strokes off every frame are unaffected.
     */
    override var bindToElement by mutableStateOf(true)
        private set

    fun toggleBindTarget() {
        bindToElement = !bindToElement
    }

    /** Lasso tool: the pen selects items and drags the selection instead of drawing. */
    override var lassoMode by mutableStateOf(false)
        private set

    /** What the lasso selected; the toolbar offers to delete it. */
    override val selection = Selection()

    fun toggleLasso() {
        lassoMode = !lassoMode
        if (lassoMode) {
            eraserMode = false
            shapeMode = null
        } else {
            selection.clear()
        }
    }

    /** Selected shape tool, or null for freehand. Turning on a shape leaves the eraser. */
    override var shapeMode by mutableStateOf<ShapeMode?>(null)
        private set

    fun toggleShape(mode: ShapeMode) {
        shapeMode = if (shapeMode == mode) null else mode
        selection.clear()
        if (shapeMode != null) {
            eraserMode = false
            lassoMode = false
        }
    }

    companion object {
        private const val MARKER = "marker"
        const val SIZE_MIN = 0.5f
        const val SIZE_MAX = 3f
        private const val ERASER_MIN_PX = 8f
        private const val ERASER_MAX_PX = 40f
        private const val BLACK = -0x1000000
        private const val ALPHA_MASK = -0x1000000
        private const val RGB_MASK = 0x00FFFFFF
    }
}
