package dev.penwise.tools

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.PathFillType
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.PathParser
import androidx.compose.ui.unit.dp

private const val ICON_SIZE = 24f

/** Material icons ships no eraser, so this is the standard tilted-block eraser in the same 24dp filled style. */
private const val ERASER_PATH =
    "M16.24,3.56l4.95,4.94c0.78,0.79 0.78,2.05 0,2.84L12,20.53a4.008,4.008 0,0 1,-5.66 0L2.81,17" +
        "c-0.78,-0.79 -0.78,-2.05 0,-2.84l10.6,-10.6c0.79,-0.78 2.05,-0.78 2.83,0z" +
        "M4.22,15.58l3.54,3.53c0.78,0.79 2.04,0.79 2.83,0l3.53,-3.53 -4.95,-4.95 -4.95,4.95z"

internal val EraserIcon: ImageVector =
    ImageVector
        .Builder("Eraser", ICON_SIZE.dp, ICON_SIZE.dp, ICON_SIZE, ICON_SIZE)
        .addPath(
            pathData = PathParser().parsePathString(ERASER_PATH).toNodes(),
            pathFillType = PathFillType.EvenOdd,
            fill = SolidColor(Color.Black),
        ).build()
