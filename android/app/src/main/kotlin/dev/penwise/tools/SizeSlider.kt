package dev.penwise.tools

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.drag
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.unit.dp

private const val SLIDER_WIDTH_DP = 160
private const val SLIDER_HEIGHT_DP = 32
private const val TRACK_DP = 4
private const val THUMB_DP = 9
private const val TRACK_COLOR = 0xFFBDBDBD
private const val THUMB_COLOR = 0xFF202124

/** Maps a touch x within [width] px (thumb-inset by [inset]) to a value in [range]. */
private fun sliderValue(
    x: Float,
    width: Float,
    inset: Float,
    range: ClosedFloatingPointRange<Float>,
): Float {
    val fraction = ((x - inset) / (width - 2 * inset).coerceAtLeast(1f)).coerceIn(0f, 1f)
    return range.start + fraction * (range.endInclusive - range.start)
}

/** Minimal horizontal slider (foundation only). */
@Composable
internal fun SizeSlider(
    value: Float,
    range: ClosedFloatingPointRange<Float>,
    onChange: (Float) -> Unit,
    modifier: Modifier = Modifier,
) {
    val currentOnChange by rememberUpdatedState(onChange)
    Canvas(
        modifier
            .size(SLIDER_WIDTH_DP.dp, SLIDER_HEIGHT_DP.dp)
            .pointerInput(range) {
                val inset = THUMB_DP.dp.toPx()
                val width = size.width.toFloat()
                awaitEachGesture {
                    val down = awaitFirstDown()
                    currentOnChange(sliderValue(down.position.x, width, inset, range))
                    drag(down.id) { change ->
                        change.consume()
                        currentOnChange(sliderValue(change.position.x, width, inset, range))
                    }
                }
            },
    ) {
        val inset = THUMB_DP.dp.toPx()
        val y = size.height / 2
        val fraction = (value - range.start) / (range.endInclusive - range.start)
        drawLine(Color(TRACK_COLOR), Offset(inset, y), Offset(size.width - inset, y), TRACK_DP.dp.toPx())
        drawCircle(Color(THUMB_COLOR), inset, Offset(inset + fraction * (size.width - 2 * inset), y))
    }
}
