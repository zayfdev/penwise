package dev.penwise.tools

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.drag
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import dev.penwise.frames.ChromeStyle

private const val SLIDER_HEIGHT_DP = 28
private const val TRACK_DP = 6
private const val THUMB_DP = 12

/** Maps a touch x within [width] px (thumb-inset by [inset]) to a value in [range]. */
internal fun sliderValue(
    x: Float,
    width: Float,
    inset: Float,
    range: ClosedFloatingPointRange<Float>,
): Float {
    val fraction = ((x - inset) / (width - 2 * inset).coerceAtLeast(1f)).coerceIn(0f, 1f)
    return range.start + fraction * (range.endInclusive - range.start)
}

/** Minimal horizontal slider (foundation only): a rounded track, its filled part and a round thumb. */
@Composable
internal fun SizeSlider(
    value: Float,
    range: ClosedFloatingPointRange<Float>,
    label: String,
    onChange: (Float) -> Unit,
    modifier: Modifier = Modifier,
) {
    val currentOnChange by rememberUpdatedState(onChange)
    val colors = ChromeStyle.colors
    Canvas(
        modifier
            .fillMaxWidth()
            .height(SLIDER_HEIGHT_DP.dp)
            .semantics { contentDescription = label }
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
        val track = TRACK_DP.dp.toPx()
        val top = (size.height - track) / 2
        val fraction = (value - range.start) / (range.endInclusive - range.start)
        val thumbX = inset + fraction * (size.width - 2 * inset)
        val round = CornerRadius(track / 2)
        drawRoundRect(colors.track, Offset(0f, top), Size(size.width, track), round)
        drawRoundRect(colors.accent, Offset(0f, top), Size(thumbX, track), round)
        drawCircle(colors.text, inset, Offset(thumbX, size.height / 2))
    }
}
