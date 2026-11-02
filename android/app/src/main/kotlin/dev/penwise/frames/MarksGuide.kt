package dev.penwise.frames

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.penwise.R
import dev.penwise.tools.Palette

private const val PANEL_PAD_DP = 8
private const val GAP_DP = 4
private const val TITLE_SP = 12
private const val TEXT_SP = 11
private const val SWATCH_DP = 10
private const val MEANING_WIDTH_DP = 260

/** One row of the guide: the mark (or colour) and what the agent reads into it. */
private class GuideEntry(
    val label: Int,
    val meaning: Int,
    val swatch: Int? = null,
)

private val MARKS =
    listOf(
        GuideEntry(R.string.mark_guide_circle, R.string.mark_guide_circle_meaning),
        GuideEntry(R.string.mark_guide_arrow, R.string.mark_guide_arrow_meaning),
        GuideEntry(R.string.mark_guide_cross, R.string.mark_guide_cross_meaning),
        GuideEntry(R.string.mark_guide_wavy, R.string.mark_guide_wavy_meaning),
        GuideEntry(R.string.mark_guide_box, R.string.mark_guide_box_meaning),
        GuideEntry(R.string.mark_guide_strike, R.string.mark_guide_strike_meaning),
        GuideEntry(R.string.mark_guide_handwriting, R.string.mark_guide_handwriting_meaning),
    )

private val COLOURS =
    listOf(
        GuideEntry(R.string.mark_colour_red, R.string.mark_colour_red_meaning, Palette.RED),
        GuideEntry(R.string.mark_colour_blue, R.string.mark_colour_blue_meaning, Palette.BLUE),
        GuideEntry(R.string.mark_colour_green, R.string.mark_colour_green_meaning, Palette.GREEN),
    )

/**
 * The drawer's cheat sheet (§5): the mark vocabulary and the colour meanings the agent reads. Recourse for a drawing
 * whose meaning the owner wants to be sure of; static text, no state.
 */
@Composable
internal fun MarksGuide(modifier: Modifier = Modifier) {
    val colors = ChromeStyle.colors
    val shape = RoundedCornerShape(ChromeStyle.PANEL_RADIUS_DP.dp)
    Column(
        modifier
            .background(colors.panel, shape)
            .border(ChromeStyle.HAIRLINE_DP.dp, colors.hairline, shape)
            .padding(PANEL_PAD_DP.dp),
        verticalArrangement = Arrangement.spacedBy(GAP_DP.dp),
    ) {
        BasicText(
            stringResource(R.string.marks_guide_title),
            style = TextStyle(fontSize = TITLE_SP.sp, color = colors.text, fontFamily = ChromeStyle.font),
        )
        for (entry in MARKS) GuideRow(entry)
        for (entry in COLOURS) GuideRow(entry)
    }
}

@Composable
private fun GuideRow(entry: GuideEntry) {
    val colors = ChromeStyle.colors
    Row(
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(GAP_DP.dp * 2),
    ) {
        entry.swatch?.let {
            Box(Modifier.size(SWATCH_DP.dp).background(Color(it), CircleShape))
        }
        BasicText(
            stringResource(entry.label),
            style = TextStyle(fontSize = TEXT_SP.sp, color = colors.text, fontFamily = ChromeStyle.font),
        )
        BasicText(
            stringResource(entry.meaning),
            style = TextStyle(fontSize = TEXT_SP.sp, color = colors.textMuted, fontFamily = ChromeStyle.font),
            modifier = Modifier.width(MEANING_WIDTH_DP.dp),
        )
    }
}
