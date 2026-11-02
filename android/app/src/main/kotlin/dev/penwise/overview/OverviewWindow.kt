package dev.penwise.overview

import android.graphics.Bitmap
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.AnimationVector1D
import androidx.compose.animation.core.CubicBezierEasing
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.pager.VerticalPager
import androidx.compose.foundation.pager.rememberPagerState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.lerp
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.graphicsLayer
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
import dev.penwise.frames.LaneKey
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch
import kotlin.math.roundToInt

private const val TITLE_SP = 15
private const val EDGE_DP = 20
private const val DOT_DP = 8
private const val DOT_GAP_DP = 10
private const val BUTTON_PAD_H_DP = 16
private const val BUTTON_PAD_V_DP = 10
private const val BUTTON_SP = 14
private val ZoomEasing = CubicBezierEasing(0.2f, 0f, 0f, 1f)

/** What the overview shows: the strips, the strip it opens on, the decks and how pictures are fetched. */
internal class OverviewWindowModel(
    val strips: List<OverviewStrip>,
    val focusStrip: Int,
    val decks: DeckState,
    val load: suspend (CardImage) -> Bitmap?,
    /** A card's bitmap if it is already in memory. */
    val peek: (CardImage) -> Bitmap?,
    /** Pictures loaded before the window opened, by card id: the opening animation needs the "now" card's. */
    val seed: Map<String, ImageBitmap>,
    val reducedMotion: Boolean,
)

/** What the overview can ask of the screen behind it. */
internal class OverviewWindowActions(
    val onDismiss: () -> Unit,
    val onToggleDeck: (LaneKey, OverviewColumn) -> Unit,
    val onEnter: (CardAction) -> Unit,
    val onPlus: (PlusAction) -> Unit,
    val onNewScreen: () -> Unit,
)

/** A live card being entered: its box when it was tapped and its picture. */
private class Entering(
    val cardId: String,
    val from: Rect?,
)

/** A tap on a card: a live one zooms to the screen first (the window fades with [progress]); others enter at once. */
private class CardEntry(
    private val bounds: Map<String, Rect>,
    private val progress: Animatable<Float, AnimationVector1D>,
    private val ms: Int,
    private val scope: CoroutineScope,
    private val onEnter: (CardAction) -> Unit,
) {
    var entering by mutableStateOf<Entering?>(null)
        private set

    fun tap(card: OverviewCard) {
        if (entering != null) return
        val action = card.action
        if (action !is CardAction.LockFrame) {
            onEnter(action)
            return
        }
        entering = Entering(card.id, bounds[card.id])
        scope.launch {
            progress.animateTo(1f, tween(ms, easing = ZoomEasing))
            onEnter(action)
        }
    }
}

/**
 * The overview (contract §3.3), a full-screen window above the ink: strips stacked vertically, one per screen, each a
 * row of columns. Entering a live card zooms it from its box to the screen (at most [OverviewMotion.ZOOM_MS]); opening
 * shrinks the screen into the "now" card. With reduced motion both are a crossfade.
 */
@Composable
internal fun OverviewWindow(
    model: OverviewWindowModel,
    actions: OverviewWindowActions,
) {
    Dialog(
        onDismissRequest = actions.onDismiss,
        properties = DialogProperties(usePlatformDefaultWidth = false, decorFitsSystemWindows = false),
    ) {
        HideBarsInDialog()
        OverviewContent(model, actions)
    }
}

@Composable
private fun OverviewContent(
    model: OverviewWindowModel,
    actions: OverviewWindowActions,
) {
    val loaded = remember { mutableStateMapOf<String, ImageBitmap>().apply { putAll(model.seed) } }
    val bounds = remember { HashMap<String, Rect>() }
    val reveal = remember { Animatable(0f) }
    val enter = remember { Animatable(0f) }
    val crossfade = OverviewMotion.zoom(model.reducedMotion) == OverviewMotion.Motion.CROSSFADE
    val scope = rememberCoroutineScope()
    val ms = if (crossfade) OverviewMotion.FADE_MS else OverviewMotion.ZOOM_MS
    val entry = remember { CardEntry(bounds, enter, ms, scope, actions.onEnter) }
    val entering = entry.entering
    LaunchedEffect(Unit) { reveal.animateTo(1f, tween(ms, easing = ZoomEasing)) }
    val env =
        StripEnv(
            decks = model.decks,
            loaded = loaded,
            load = model.load,
            peek = model.peek,
            onLoaded = { id, bitmap -> loaded[id] = bitmap },
            onToggleDeck = actions.onToggleDeck,
            onTap = entry::tap,
            onBounds = { id, rect -> bounds[id] = rect },
            onPlus = actions.onPlus,
        )
    BoxWithConstraints(Modifier.fillMaxSize().dottedCanvas()) {
        val full = Rect(0f, 0f, constraints.maxWidth.toFloat(), constraints.maxHeight.toFloat())
        val widthDp = maxWidth.value
        val heightDp = maxHeight.value
        Box(Modifier.fillMaxSize().graphicsLayer { alpha = reveal.value * (1f - enter.value) }) {
            Strips(model, env, actions, widthDp, heightDp)
        }
        val nowCard =
            model.strips
                .getOrNull(model.focusStrip)
                ?.live
                ?.id
        val ghost = entering?.let { it.cardId to it.from } ?: (nowCard to nowCard?.let { bounds[it] })
        val bitmap = ghost.first?.let { loaded[it] }
        val card = ghost.second
        if (bitmap != null && card != null && (entering != null || reveal.value < 1f)) {
            val progress = if (entering != null) enter.value else 1f - reveal.value
            val fade = if (entering != null) enter.value else 1f - reveal.value
            ZoomGhost(bitmap, card, full, progress, fade, model.reducedMotion)
        }
    }
}

@Composable
private fun Strips(
    model: OverviewWindowModel,
    env: StripEnv,
    actions: OverviewWindowActions,
    widthDp: Float,
    heightDp: Float,
) {
    val pager = rememberPagerState(initialPage = model.focusStrip) { model.strips.size }
    VerticalPager(state = pager, modifier = Modifier.fillMaxSize()) { index ->
        val strip = model.strips[index]
        val live = strip.live
        val aspect = live.cssWidth.toFloat() / live.cssHeight.coerceAtLeast(1)
        StripRow(strip, OverviewMetrics.of(widthDp, heightDp, aspect), env, Modifier.fillMaxSize())
    }
    Box(Modifier.fillMaxSize().safeDrawingPadding().padding(EDGE_DP.dp)) {
        BasicText(
            model.strips
                .getOrNull(pager.currentPage)
                ?.title
                .orEmpty(),
            style = TextStyle(color = ChromeStyle.dark.text, fontSize = TITLE_SP.sp, fontFamily = ChromeStyle.font),
            modifier = Modifier.align(Alignment.TopStart),
        )
        OverviewButton("Close", actions.onDismiss, Modifier.align(Alignment.TopEnd))
        OverviewButton("New screen", actions.onNewScreen, Modifier.align(Alignment.BottomCenter))
        StripDots(model.strips.size, pager.currentPage, Modifier.align(Alignment.CenterEnd))
    }
}

@Composable
private fun StripDots(
    count: Int,
    current: Int,
    modifier: Modifier = Modifier,
) {
    if (count < 2) return
    Column(modifier, verticalArrangement = Arrangement.spacedBy(DOT_GAP_DP.dp)) {
        repeat(count) { index ->
            Box(
                Modifier
                    .size(DOT_DP.dp)
                    .background(
                        if (index ==
                            current
                        ) {
                            ChromeStyle.dark.text
                        } else {
                            ChromeStyle.dark.hairline
                        },
                        CircleShape,
                    ),
            )
        }
    }
}

@Composable
private fun OverviewButton(
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
            .padding(horizontal = BUTTON_PAD_H_DP.dp, vertical = BUTTON_PAD_V_DP.dp),
    ) {
        BasicText(
            label,
            style = TextStyle(color = ChromeStyle.dark.text, fontSize = BUTTON_SP.sp, fontFamily = ChromeStyle.font),
        )
    }
}

/**
 * The picture that travels between the screen and a card: at [progress] 0 it fills [card]'s box, at 1 the whole
 * [full] screen (entering) - run backwards when the overview opens. Reduced motion keeps it full-screen and fades it.
 */
@Composable
private fun ZoomGhost(
    bitmap: ImageBitmap,
    card: Rect,
    full: Rect,
    progress: Float,
    fade: Float,
    reduced: Boolean,
) {
    val rect = if (reduced) full else lerp(card, full, progress)
    Image(
        bitmap = bitmap,
        contentDescription = null,
        contentScale = ContentScale.Crop,
        alignment = Alignment.TopCenter,
        modifier =
            Modifier
                .fillMaxSize()
                .alpha(if (reduced) fade else 1f)
                .layout { measurable, constraints ->
                    val placeable =
                        measurable.measure(
                            Constraints.fixed(
                                rect.width.roundToInt().coerceAtLeast(1),
                                rect.height.roundToInt().coerceAtLeast(1),
                            ),
                        )
                    layout(constraints.maxWidth, constraints.maxHeight) {
                        placeable.place(rect.left.roundToInt(), rect.top.roundToInt())
                    }
                },
    )
}
