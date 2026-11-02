package dev.penwise.tools

import android.graphics.Bitmap
import android.graphics.Canvas
import android.util.LruCache
import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.core.graphics.createBitmap
import dev.penwise.core.InkStroke
import dev.penwise.ink.GeometryPainter
import kotlin.math.sin

/**
 * A brush's preview: a short S-curve drawn by the ink engine at the brush's own width, dynamics and texture, so a
 * fineliner looks thin and a highlighter looks wet before you pick it. [argb] is the preview colour (the flyout draws
 * it in the chrome's text colour, so it reads on the panel).
 */
@Composable
internal fun BrushPreview(
    preset: Int,
    argb: Int,
    modifier: Modifier = Modifier,
    size: Dp = 40.dp,
) {
    val density = LocalDensity.current
    val bitmap = remember(preset, argb, size, density) { brushBitmap(preset, argb, size, density) }
    if (bitmap != null) {
        Image(bitmap = bitmap, contentDescription = null, modifier = modifier.size(size).padding(PREVIEW_PAD_DP.dp))
    } else {
        Box(modifier.size(size))
    }
}

private const val PREVIEW_PAD_DP = 5
private const val PREVIEW_SIDE_PX = 96
private const val SAMPLE_COUNT = 24
private const val ICON_SEED = 7_654_321L
private const val ICON_SIZE_SCALE = 0.7f
private const val SAMPLE_INTERVAL_US = 4_000
private const val SAMPLE_STRIDE = 5
private const val TILT_SLOT = 3
private const val AZIMUTH_SLOT = 4
private const val CURVE_MARGIN = 0.2f
private const val CURVE_MIDDLE = 0.5f
private const val CURVE_AMPLITUDE = 0.18f
private const val CURVE_TURNS = 6.283_185f
private const val CURVE_TAPER_GAIN = 1.15f
private const val MIN_TAPER = 0.12f
private const val MAX_TAPER = 1f
private val ICON_CACHE = LruCache<String, Bitmap>(64)

/** Renders (and caches) one brush's preview squiggle. */
private fun brushBitmap(
    preset: Int,
    argb: Int,
    size: Dp,
    density: Density,
): ImageBitmap? {
    val key = "p$preset-a$argb-s${size.hashCode()}-d${density.density}"
    ICON_CACHE.get(key)?.let { return it.asImageBitmap() }
    val pixels = with(density) { (size.toPx() - PREVIEW_PAD_DP * 2 * density.density).toInt().coerceAtLeast(24) }
    val side = minOf(pixels, PREVIEW_SIDE_PX)
    val stroke = previewStroke(preset, argb, side.toFloat()) ?: return null
    val geometry =
        stroke.use {
            it.finish()
        }
    val bitmap = createBitmap(side, side)
    val painter = GeometryPainter()
    painter.draw(Canvas(bitmap), geometry)
    ICON_CACHE.put(key, bitmap)
    return bitmap.asImageBitmap()
}

/** A gentle S with a pressure ramp, laid out in a `side`-square box. */
private fun previewStroke(
    preset: Int,
    argb: Int,
    side: Float,
): InkStroke? {
    val samples = FloatArray(SAMPLE_COUNT * SAMPLE_STRIDE)
    val times = IntArray(SAMPLE_COUNT)
    val margin = side * CURVE_MARGIN
    val span = side - margin * 2
    for (i in 0 until SAMPLE_COUNT) {
        val t = i.toFloat() / (SAMPLE_COUNT - 1)
        val taper = (sin(t * Math.PI.toFloat()) * CURVE_TAPER_GAIN).coerceIn(MIN_TAPER, MAX_TAPER)
        samples[i * SAMPLE_STRIDE] = margin + span * t
        samples[i * SAMPLE_STRIDE + 1] = side * CURVE_MIDDLE + sin(t * CURVE_TURNS) * side * CURVE_AMPLITUDE
        samples[i * SAMPLE_STRIDE + 2] = taper
        samples[i * SAMPLE_STRIDE + TILT_SLOT] = 0f
        samples[i * SAMPLE_STRIDE + AZIMUTH_SLOT] = 0f
        times[i] = i * SAMPLE_INTERVAL_US
    }
    val stroke =
        runCatching { InkStroke(preset, ICON_SEED, argb, ICON_SIZE_SCALE) }.getOrNull() ?: return null
    val pushed = runCatching { stroke.push(samples, times, SAMPLE_COUNT) }
    if (pushed.isFailure) {
        stroke.close()
        return null
    }
    return stroke
}
