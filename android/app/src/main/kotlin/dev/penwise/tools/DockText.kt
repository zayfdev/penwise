package dev.penwise.tools

import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.sp
import dev.penwise.frames.ChromeStyle

/** Text in the chrome: one family from [ChromeStyle], so the fonts swap in one place. */
@Composable
internal fun DockText(
    text: String,
    color: Color,
    size: Int,
    modifier: Modifier = Modifier,
    weight: FontWeight = FontWeight.Normal,
    family: FontFamily = ChromeStyle.font,
) {
    BasicText(
        text,
        modifier,
        style = TextStyle(color = color, fontSize = size.sp, fontWeight = weight, fontFamily = family),
        maxLines = 1,
    )
}
