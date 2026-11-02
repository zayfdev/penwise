package dev.penwise.overview

import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.calculateCentroid
import androidx.compose.foundation.gestures.calculatePan
import androidx.compose.foundation.gestures.calculateZoom
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.geometry.isSpecified
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.TransformOrigin
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.PointerType
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.input.pointer.positionChanged
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.layout.layout
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import dev.penwise.frames.ChromeStyle
import dev.penwise.frames.VariantSwitcher

private const val MARGIN_DP = 16
private const val TEXT_SP = 14

/** What the read-only viewer can ask of the screen behind it. */
internal class ViewerActions(
    val onShow: (item: String) -> Unit,
    val onBackToNow: () -> Unit,
    val onBack: () -> Unit,
)

/**
 * A past picture, full screen and read-only (contract §3.3): the pen and one finger do not edit anything, two fingers
 * zoom and pan. The switcher shows the generation (OG, A, B..., a check on the pick) and "Back to now" leaves for the
 * live frame. A separate window, so it sits above the ink surface; the system Back returns to the overview.
 */
@Composable
internal fun HistoryViewer(
    view: HistoryView,
    images: OverviewImages,
    canGoNow: Boolean,
    actions: ViewerActions,
) {
    Dialog(
        onDismissRequest = actions.onBack,
        properties = DialogProperties(usePlatformDefaultWidth = false, decorFitsSystemWindows = false),
    ) {
        HideBarsInDialog()
        BoxWithConstraints(Modifier.fillMaxSize().background(OverviewLook.canvas)) {
            val picture = HistorySwitcher.picture(view)
            val widthPx = constraints.maxWidth
            val bitmap by produceState<ImageBitmap?>(null, picture.asset, widthPx) {
                value = images.picture(picture.asset, widthPx)?.asImageBitmap()
            }
            bitmap?.let {
                val size = ViewerSize.fitted(it.width, it.height, widthPx.toFloat(), constraints.maxHeight.toFloat())
                ZoomablePicture(it, size, key = picture.item)
            }
            Box(Modifier.fillMaxSize().safeDrawingPadding().padding(MARGIN_DP.dp)) {
                BasicText(
                    "Round ${view.generation.round} · ${HistorySwitcher.of(
                        view,
                    ).items.firstOrNull { it.id == view.shown }?.label.orEmpty()}",
                    style =
                        TextStyle(
                            color = ChromeStyle.dark.text,
                            fontSize = TEXT_SP.sp,
                            fontFamily = ChromeStyle.font,
                        ),
                    modifier = Modifier.align(Alignment.TopStart),
                )
                if (canGoNow) ViewerButton("Back to now", actions.onBackToNow, Modifier.align(Alignment.TopEnd))
                VariantSwitcher(
                    state = HistorySwitcher.of(view),
                    onSelect = actions.onShow,
                    onUseThis = {},
                    modifier = Modifier.align(Alignment.BottomCenter),
                )
            }
        }
    }
}

/** The picture at [size], moved only by two-finger (or one-finger) transform gestures; the stylus does nothing. */
@Composable
private fun ZoomablePicture(
    bitmap: ImageBitmap,
    size: ViewerSize,
    key: String,
) {
    var transform by remember(key) { mutableStateOf(ViewerTransform()) }
    val box = size.box()
    Box(
        Modifier
            .fillMaxSize()
            .clipToBounds()
            .pointerInput(size, key) {
                awaitEachGesture {
                    awaitFirstDown(requireUnconsumed = false)
                    do {
                        val event = awaitPointerEvent()
                        val pen = event.changes.any { it.type == PointerType.Stylus || it.type == PointerType.Eraser }
                        val centre = event.calculateCentroid(useCurrent = false)
                        if (!pen && centre.isSpecified) {
                            val pan = event.calculatePan()
                            transform =
                                transform.gesture(
                                    ViewerGesture(event.calculateZoom(), centre.x, centre.y, pan.x, pan.y),
                                    size,
                                )
                            event.changes.forEach { if (it.positionChanged()) it.consume() }
                        }
                    } while (event.changes.any { it.pressed })
                }
            },
    ) {
        Image(
            bitmap = bitmap,
            contentDescription = null,
            contentScale = ContentScale.FillBounds,
            modifier =
                Modifier
                    // Laid out at the screen's top-left: the transform counts from there (a centred box would put
                    // the top of a tall picture out of reach).
                    .layout { measurable, constraints ->
                        val placeable = measurable.measure(Constraints.fixed(box.width, box.height))
                        layout(constraints.maxWidth, constraints.maxHeight) { placeable.place(box.left, box.top) }
                    }.graphicsLayer {
                        transformOrigin = TransformOrigin(0f, 0f)
                        scaleX = transform.scale
                        scaleY = transform.scale
                        translationX = transform.x
                        translationY = transform.y
                    },
        )
    }
}

@Composable
private fun ViewerButton(
    label: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val shape = RoundedCornerShape(ChromeStyle.BUTTON_RADIUS_DP.dp)
    Box(
        modifier
            .clip(shape)
            .background(ChromeStyle.dark.panel)
            .border(ChromeStyle.HAIRLINE_DP.dp, ChromeStyle.dark.hairline, shape)
            .clickable(role = Role.Button, onClick = onClick)
            .padding(horizontal = MARGIN_DP.dp, vertical = BUTTON_PAD_DP.dp),
    ) {
        BasicText(
            label,
            style = TextStyle(color = ChromeStyle.dark.text, fontSize = TEXT_SP.sp, fontFamily = ChromeStyle.font),
        )
    }
}

private const val BUTTON_PAD_DP = 10
