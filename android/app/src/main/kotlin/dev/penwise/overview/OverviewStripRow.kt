package dev.penwise.overview

import android.graphics.Bitmap
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.CubicBezierEasing
import androidx.compose.animation.core.tween
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.pager.HorizontalPager
import androidx.compose.foundation.pager.PageSize
import androidx.compose.foundation.pager.rememberPagerState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.unit.dp
import dev.penwise.frames.LaneKey

private const val COLUMN_TOP_DP = 76
private const val COLUMN_BOTTOM_DP = 96
private const val COLUMN_GAP_DP = 12
private val Springy = CubicBezierEasing(0.34f, 1.56f, 0.64f, 1f)

/** What every strip shares: the decks, the bitmaps cards have loaded, and the calls a card or a `+` makes. */
internal class StripEnv(
    val decks: DeckState,
    val loaded: Map<String, ImageBitmap>,
    val load: suspend (CardImage) -> Bitmap?,
    val peek: (CardImage) -> Bitmap?,
    val onLoaded: (cardId: String, bitmap: ImageBitmap) -> Unit,
    val onToggleDeck: (LaneKey, OverviewColumn) -> Unit,
    val onTap: (OverviewCard) -> Unit,
    val onBounds: (cardId: String, bounds: Rect) -> Unit,
    val onPlus: (PlusAction) -> Unit,
)

/** The column the strip is on, by key, so a refetch that adds columns on the left does not move the owner. */
private class FocusKey(
    var key: String? = OverviewColumn.NOW_KEY,
)

/**
 * One strip (contract §3.3): its columns side by side, one at a time snapped to the centre with the neighbours
 * peeking. The strip opens on its "now" column.
 */
@Composable
internal fun StripRow(
    strip: OverviewStrip,
    metrics: OverviewMetrics,
    env: StripEnv,
    modifier: Modifier = Modifier,
) {
    val keys = strip.columns.map { it.key }
    val pager = rememberPagerState(initialPage = strip.nowIndex) { strip.columns.size }
    val focus = remember { FocusKey() }
    val latestKeys by rememberUpdatedState(keys)
    LaunchedEffect(pager) { snapshotFlow { pager.settledPage }.collect { focus.key = latestKeys.getOrNull(it) } }
    LaunchedEffect(keys) {
        val target = SnapIndex.after(focus.key, keys, pager.currentPage)
        if (target != pager.currentPage) pager.scrollToPage(target)
    }
    val size = CardSize(metrics.cardWidth.dp, metrics.cardHeight.dp)
    HorizontalPager(
        state = pager,
        modifier = modifier,
        contentPadding = PaddingValues(horizontal = metrics.sidePadding.dp),
        pageSize = PageSize.Fixed(metrics.cardWidth.dp),
        pageSpacing = metrics.gutter.dp,
        key = { strip.columns.getOrNull(it)?.key ?: it },
    ) { index ->
        strip.columns.getOrNull(index)?.let { ColumnView(strip.lane, it, size, env) }
    }
}

@Composable
private fun ColumnView(
    lane: LaneKey,
    column: OverviewColumn,
    size: CardSize,
    env: StripEnv,
) {
    val content = remember(column, lane) { ColumnCards.of(column, lane) }
    Column(
        Modifier
            .fillMaxHeight()
            .verticalScroll(rememberScrollState())
            .padding(top = COLUMN_TOP_DP.dp, bottom = COLUMN_BOTTOM_DP.dp),
        verticalArrangement = Arrangement.spacedBy(COLUMN_GAP_DP.dp),
    ) {
        if (column == OverviewColumn.Plus) {
            PlusEnd(lane, size, env)
            return@Column
        }
        content.head?.let { LoadedCard(it, size, env) }
        val summary = content.summary ?: return@Column
        val expanded = env.decks.isExpanded(lane, column)
        val toggle = { env.onToggleDeck(lane, column) }
        AnimatedVisibility(
            visible = !expanded,
            enter = fadeIn(tween(DECK_MS)),
            exit = fadeOut(tween(DECK_MS)),
        ) { DeckPile(summary, size.width, toggle) }
        AnimatedVisibility(
            visible = expanded,
            enter = expandVertically(tween(DECK_MS, easing = Springy)) + fadeIn(tween(DECK_MS)),
            exit = shrinkVertically(tween(DECK_MS)) + fadeOut(tween(DECK_MS)),
        ) {
            Column(verticalArrangement = Arrangement.spacedBy(COLUMN_GAP_DP.dp)) {
                DeckHeader(summary, toggle)
                for (variant in content.variants) LoadedCard(variant, size, env)
            }
        }
    }
}

@Composable
private fun PlusEnd(
    lane: LaneKey,
    size: CardSize,
    env: StripEnv,
) {
    var open by remember { mutableStateOf(false) }
    PlusCard(size, PlusMenu.entries(lane), open, onToggle = { open = !open }, onPick = env.onPlus)
}

/** A card that loads its picture when it first composes. */
@Composable
private fun LoadedCard(
    card: OverviewCard,
    size: CardSize,
    env: StripEnv,
) {
    // A picture already in memory shows in the first frame (the opening animation needs it).
    val cached = remember(card.id, card.image) { env.peek(card.image)?.asImageBitmap() }
    LaunchedEffect(card.id, card.image) {
        CardLoading.showThenRefresh(
            peek = { cached },
            load = { env.load(card.image)?.asImageBitmap() },
            show = { env.onLoaded(card.id, it) },
        )
    }
    CardView(card, env.loaded[card.id] ?: cached, size, onTap = {
        env.onTap(card)
    }, onBounds = { env.onBounds(card.id, it) })
}

private const val DECK_MS = OverviewMotion.DECK_MS
