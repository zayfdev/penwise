package dev.penwise.overview

import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.layout.boundsInRoot
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.penwise.frames.ChromeStyle

private const val TITLE_SP = 12
private const val CARD_RADIUS_DP = 14
private const val TITLE_GAP_DP = 6
private const val BADGE_DP = 22
private const val BADGE_INSET_DP = 8
private const val PILE_HEIGHT_DP = 64
private const val PILE_LAYERS = 3
private const val PILE_STEP_DP = 7
private const val PILE_INSET_DP = 10
private const val PLACEHOLDER_SP = 18

/** The size of a strip's cards in dp. */
internal class CardSize(
    val width: Dp,
    val height: Dp,
)

private fun titleStyle(color: Color = ChromeStyle.textMuted) =
    TextStyle(color = color, fontSize = TITLE_SP.sp, fontFamily = ChromeStyle.font)

/** A card with its small title above it; a tap enters it. [onBounds] reports the card's box for the zoom. */
@Composable
internal fun CardView(
    card: OverviewCard,
    bitmap: ImageBitmap?,
    size: CardSize,
    onTap: () -> Unit,
    onBounds: (Rect) -> Unit,
    modifier: Modifier = Modifier,
) {
    Column(modifier.width(size.width), verticalArrangement = Arrangement.spacedBy(TITLE_GAP_DP.dp)) {
        BasicText(card.title, style = titleStyle(), maxLines = 1)
        val shape = RoundedCornerShape(CARD_RADIUS_DP.dp)
        Box(
            Modifier
                .size(size.width, size.height)
                .onGloballyPositioned { onBounds(it.boundsInRoot()) }
                .clip(shape)
                .background(OverviewLook.cardFill)
                .border(ChromeStyle.HAIRLINE_DP.dp, ChromeStyle.hairline, shape)
                .clickable(role = Role.Button, onClick = onTap),
        ) {
            CardPicture(card.image, bitmap)
            if (card.checked) CheckBadge(Modifier.align(Alignment.TopEnd).padding(BADGE_INSET_DP.dp))
        }
    }
}

/** The card's picture: the bitmap's top (what identifies a page), or the sketch page's light placeholder. */
@Composable
private fun CardPicture(
    image: CardImage,
    bitmap: ImageBitmap?,
) {
    when {
        image is CardImage.Placeholder -> {
            Box(Modifier.fillMaxSize().background(OverviewLook.paper), contentAlignment = Alignment.Center) {
                BasicText(
                    image.title,
                    style =
                        TextStyle(
                            color = OverviewLook.paperText,
                            fontSize = PLACEHOLDER_SP.sp,
                            fontFamily = ChromeStyle.font,
                        ),
                )
            }
        }

        bitmap != null -> {
            Image(
                bitmap = bitmap,
                contentDescription = null,
                modifier = Modifier.fillMaxSize(),
                contentScale = ContentScale.Crop,
                alignment = Alignment.TopCenter,
            )
        }
    }
}

@Composable
private fun CheckBadge(modifier: Modifier = Modifier) {
    Box(
        modifier
            .size(BADGE_DP.dp)
            .background(ChromeStyle.panel, CircleShape)
            .border(ChromeStyle.HAIRLINE_DP.dp, ChromeStyle.accent, CircleShape),
        contentAlignment = Alignment.Center,
    ) {
        BasicText("✓", style = titleStyle(ChromeStyle.text))
    }
}

/** The collapsed deck under the OG card: a pile with "3 variations · picked B"; a tap expands it. */
@Composable
internal fun DeckPile(
    summary: String,
    width: Dp,
    onTap: () -> Unit,
) {
    Box(Modifier.width(width).height(PILE_HEIGHT_DP.dp).clickable(role = Role.Button, onClick = onTap)) {
        for (depth in PILE_LAYERS - 1 downTo 0) {
            val shape = RoundedCornerShape(CARD_RADIUS_DP.dp)
            Box(
                Modifier
                    .fillMaxWidth()
                    .padding(horizontal = (depth * PILE_INSET_DP).dp)
                    .offset(y = (depth * PILE_STEP_DP).dp)
                    .height((PILE_HEIGHT_DP - PILE_LAYERS * PILE_STEP_DP).dp)
                    .background(OverviewLook.cardFill, shape)
                    .border(ChromeStyle.HAIRLINE_DP.dp, ChromeStyle.hairline, shape),
            )
        }
        Box(Modifier.fillMaxWidth().height((PILE_HEIGHT_DP - PILE_LAYERS * PILE_STEP_DP).dp), Alignment.Center) {
            BasicText(summary, style = titleStyle(ChromeStyle.text))
        }
    }
}

/** The header of an expanded deck: the same summary with a way to fold it again. */
@Composable
internal fun DeckHeader(
    summary: String,
    onTap: () -> Unit,
) {
    BasicText(
        "$summary  ▴",
        style = titleStyle(ChromeStyle.text),
        modifier = Modifier.clickable(role = Role.Button, onClick = onTap).padding(vertical = TITLE_GAP_DP.dp),
    )
}

/** The `+` at a strip's end: a card-sized add button that opens its menu in place. */
@Composable
internal fun PlusCard(
    size: CardSize,
    entries: List<PlusAction>,
    open: Boolean,
    onToggle: () -> Unit,
    onPick: (PlusAction) -> Unit,
) {
    Column(Modifier.width(size.width), verticalArrangement = Arrangement.spacedBy(TITLE_GAP_DP.dp)) {
        val shape = RoundedCornerShape(CARD_RADIUS_DP.dp)
        BasicText("Add", style = titleStyle())
        Box(
            Modifier
                .size(size.width, size.height)
                .clip(shape)
                .border(ChromeStyle.HAIRLINE_DP.dp, ChromeStyle.hairline, shape)
                .clickable(role = Role.Button, onClick = onToggle),
            contentAlignment = Alignment.Center,
        ) {
            BasicText(
                "+",
                style = TextStyle(color = ChromeStyle.text, fontSize = PLUS_SP.sp, fontFamily = ChromeStyle.font),
            )
        }
        if (open) {
            for (entry in entries) {
                Box(
                    Modifier
                        .fillMaxWidth()
                        .clip(RoundedCornerShape(ChromeStyle.BUTTON_RADIUS_DP.dp))
                        .background(ChromeStyle.panel)
                        .border(
                            ChromeStyle.HAIRLINE_DP.dp,
                            ChromeStyle.hairline,
                            RoundedCornerShape(ChromeStyle.BUTTON_RADIUS_DP.dp),
                        ).clickable(role = Role.Button) { onPick(entry) }
                        .padding(PLUS_PAD_DP.dp),
                ) {
                    BasicText(PlusMenu.label(entry), style = titleStyle(ChromeStyle.text))
                }
            }
        }
    }
}

private const val PLUS_SP = 40
private const val PLUS_PAD_DP = 12
