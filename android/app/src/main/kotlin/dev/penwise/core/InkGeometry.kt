package dev.penwise.core

/**
 * Stroke geometry from the ink engine, in view px.
 *
 * Outline brushes fill [outline] (nonzero); stamp brushes draw [dabs]. Composite the whole stroke once at
 * [opacity]; [multiply] strokes (highlighter) use multiply blend and are drawn under other ink.
 */
class InkGeometry(
    val isStamps: Boolean,
    /** x,y pairs, all contours concatenated. */
    val outline: FloatArray,
    /** Exclusive end index (in points) of each contour. */
    val contourEnds: IntArray,
    /** 9 floats per dab: x, y, radiusX, radiusY, angleRad, opacity, hardness, grainStrength, grainLevel. */
    val dabs: FloatArray,
    /** Stroke color including alpha. */
    val argb: Int,
    /** Stroke-level opacity cap. */
    val opacity: Float,
    /** Highlighter: multiply blend, drawn under other ink. */
    val multiply: Boolean,
    /** Grain texture scale of the brush; 0 when the brush has no grain. */
    val grainScale: Float = 0f,
    /** Grain edge softness of the brush. */
    val grainSoftness: Float = 0f,
)
