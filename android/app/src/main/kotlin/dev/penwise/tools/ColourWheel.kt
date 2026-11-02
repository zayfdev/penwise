package dev.penwise.tools

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.drag
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import dev.penwise.frames.ChromeStyle

private const val RING_DP = 22
private const val SQUARE_DP = 104
private const val SQUARE_RADIUS_DP = 10
private const val HUE_STEP = 30
private const val FULL_TURN = 360
private const val RGB_MASK = 0xFFFFFF
private const val OPAQUE = -0x1000000
private const val RING_THUMB_DP = 10f
private const val SQUARE_THUMB_DP = 7f
private const val THUMB_LINE_DP = 3f
private const val THUMB_HALO = 0x66000000

/**
 * A hue ring with a saturation/value square inside it, for any colour. [rgb] is the colour shown; dragging the ring
 * changes the hue, dragging the square the saturation and value, and each change goes to [onPick] as `0xRRGGBB`.
 * [onEnd] fires when a drag ends, once per pick. The hue is kept while the colour is a grey (where RGB has none), and
 * the ring shows no marker for a colour without a hue.
 */
@Composable
internal fun ColourWheel(
    rgb: Int,
    onPick: (Int) -> Unit,
    onEnd: () -> Unit,
    label: String,
    side: Dp,
    modifier: Modifier = Modifier,
) {
    val density = LocalDensity.current
    val geometry =
        remember(side, density) {
            with(density) { WheelGeometry(side.toPx(), RING_DP.dp.toPx(), SQUARE_DP.dp.toPx()) }
        }
    val colors = ChromeStyle.colors
    // The wheel position the owner is dragging, while it still shows the colour handed in; else read it from [rgb].
    var dragged by remember { mutableStateOf<Hsv?>(null) }
    val hsv = dragged?.takeIf { ColourMath.toRgb(it) == (rgb and RGB_MASK) } ?: ColourMath.fromRgb(rgb)
    val currentHsv by rememberUpdatedState(hsv)
    val currentPick by rememberUpdatedState(onPick)
    val currentEnd by rememberUpdatedState(onEnd)
    Canvas(
        modifier
            .size(side)
            .semantics { contentDescription = label }
            .pointerInput(geometry) {
                awaitEachGesture {
                    val down = awaitFirstDown()
                    val zone = geometry.zoneAt(down.position.x, down.position.y)
                    if (zone == WheelZone.NONE) return@awaitEachGesture

                    fun apply(at: Offset) {
                        val next =
                            if (zone == WheelZone.RING) {
                                ColourMath.withHue(currentHsv, geometry.hueAt(at.x, at.y))
                            } else {
                                currentHsv.copy(
                                    saturation = geometry.saturationAt(at.x),
                                    value = geometry.valueAt(at.y),
                                )
                            }
                        dragged = next
                        currentPick(ColourMath.toRgb(next))
                    }
                    apply(down.position)
                    drag(down.id) { change ->
                        change.consume()
                        apply(change.position)
                    }
                    currentEnd()
                }
            },
    ) {
        drawWheel(geometry, hsv, colors.well)
    }
}

private fun DrawScope.drawWheel(
    geometry: WheelGeometry,
    hsv: Hsv,
    well: Color,
) {
    val centre = Offset(geometry.side / 2, geometry.side / 2)
    val stops = (0..FULL_TURN step HUE_STEP).map { Color(ColourMath.toRgb(Hsv(it.toFloat(), 1f, 1f)) or OPAQUE) }
    drawCircle(
        Brush.sweepGradient(stops, centre),
        radius = geometry.ringRadius,
        center = centre,
        style = Stroke(geometry.ringWidth),
    )
    drawCircle(well, radius = geometry.ringRadius - geometry.ringWidth / 2, center = centre)
    val origin = geometry.squareOrigin
    val box = Size(geometry.squareSide, geometry.squareSide)
    val round = CornerRadius(SQUARE_RADIUS_DP.dp.toPx())
    val pure = Color(ColourMath.toRgb(hsv.copy(saturation = 1f, value = 1f)) or OPAQUE)
    drawRoundRect(
        Brush.horizontalGradient(listOf(Color.White, pure), origin.x, origin.x + box.width),
        origin,
        box,
        round,
    )
    drawRoundRect(
        Brush.verticalGradient(listOf(Color.Transparent, Color.Black), origin.y, origin.y + box.height),
        origin,
        box,
        round,
    )
    // Black and greys have no hue, so the ring shows no marker for them instead of one parked on red.
    if (ColourMath.hasHue(hsv)) thumb(geometry.ringPoint(hsv.hue), RING_THUMB_DP.dp.toPx(), pure)
    thumb(geometry.squarePoint(hsv.saturation, hsv.value), SQUARE_THUMB_DP.dp.toPx(), null)
}

/** A round handle: a white ring over a dark halo (so it shows on light and dark), filled with [fill] when given. */
private fun DrawScope.thumb(
    at: Offset,
    radius: Float,
    fill: Color?,
) {
    val line = THUMB_LINE_DP.dp.toPx()
    fill?.let { drawCircle(it, radius, at) }
    drawCircle(Color(THUMB_HALO), radius + line / 2 + 1.dp.toPx(), at, style = Stroke(line + 2.dp.toPx()))
    drawCircle(Color.White, radius, at, style = Stroke(line))
}
