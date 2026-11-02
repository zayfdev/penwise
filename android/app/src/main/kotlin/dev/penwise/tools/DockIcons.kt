package dev.penwise.tools

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.PathBuilder
import androidx.compose.ui.graphics.vector.PathParser
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp
import kotlin.math.PI
import kotlin.math.cos
import kotlin.math.sin

/**
 * The dock's icons: one hand-drawn set of stroked outlines, 1.6 units wide with round caps and joins on a 20, 22 or 24
 * unit grid (drawn at that many dp, so the weight is the same everywhere). They are black placeholders; the button
 * tints them. No icon font and no icon library.
 */
internal object DockIcons {
    val Pen =
        icon(
            "Pen",
            GRID_MEDIUM,
            "M4 18l1.2-4.6L14.6 4a1.9 1.9 0 0 1 2.7 0l.7.7a1.9 1.9 0 0 1 0 2.7L8.6 16.8Z",
            "M12.8 5.8l3.4 3.4",
        )
    val Marker =
        icon(
            "Marker",
            GRID_MEDIUM,
            "M6 15.5 13.8 5a2 2 0 0 1 2.8-.4l.8.6a2 2 0 0 1 .4 2.8L10 18.4",
            "M6 15.5 4.5 19h4",
        )
    val Eraser =
        icon(
            "Eraser",
            GRID_MEDIUM,
            "M8.2 18h10",
            "M5 13.8 12.6 6.2a2 2 0 0 1 2.8 0l2.4 2.4a2 2 0 0 1 0 2.8L11.6 17.6a2 2 0 0 1-1.4.6H8.6a2 2 0 0 1" +
                "-1.4-.6L5 15.4a1.2 1.2 0 0 1 0-1.6Z",
            "M9 10l5 5",
        )
    val Shapes =
        icon(
            "Shapes",
            GRID_MEDIUM,
            "M5.1 3.5h4.8a1.6 1.6 0 0 1 1.6 1.6v4.8a1.6 1.6 0 0 1-1.6 1.6H5.1a1.6 1.6 0 0 1-1.6-1.6V5.1a1.6 1.6" +
                " 0 0 1 1.6-1.6Z",
            "M18.7 14.5a4.2 4.2 0 1 1-8.4 0a4.2 4.2 0 0 1 8.4 0Z",
        )
    val Select =
        icon("Select", GRID_MEDIUM, "M6.5 15.2 5 19") {
            dashedEllipse(cx = 11f, cy = 9.5f, rx = 7.5f, ry = 6f)
        }
    val Delete =
        icon(
            "Delete",
            GRID_MEDIUM,
            "M5 6.5h12",
            "M8.5 6.5V5a1 1 0 0 1 1-1h3a1 1 0 0 1 1 1v1.5",
            "M6.5 6.5l.7 10.2a1.6 1.6 0 0 0 1.6 1.5h4.4a1.6 1.6 0 0 0 1.6-1.5l.7-10.2",
            "M9.3 9.5v5M12.7 9.5v5",
        )
    val Element =
        icon(
            "Element",
            GRID_SMALL,
            "M16 10a6 6 0 1 1-12 0a6 6 0 0 1 12 0Z",
            "M10 1.5v3M10 15.5v3M1.5 10h3M15.5 10h3",
            "M11.8 10a1.8 1.8 0 1 1-3.6 0a1.8 1.8 0 0 1 3.6 0Z",
        )
    val Undo = icon("Undo", GRID_SMALL, "M7 5 3.5 8.5 7 12", "M3.5 8.5h8a4.5 4.5 0 0 1 0 9H9")
    val Redo = icon("Redo", GRID_SMALL, "M13 5l3.5 3.5L13 12", "M16.5 8.5h-8a4.5 4.5 0 0 0 0 9H11")
    val More =
        icon(
            "More",
            GRID_SMALL,
            "M6.1 10a1.6 1.6 0 1 1-3.2 0a1.6 1.6 0 0 1 3.2 0Z",
            "M11.6 10a1.6 1.6 0 1 1-3.2 0a1.6 1.6 0 0 1 3.2 0Z",
            "M17.1 10a1.6 1.6 0 1 1-3.2 0a1.6 1.6 0 0 1 3.2 0Z",
        )

    val Line = icon("Line", GRID_LARGE, "M5 19 19 5")
    val Arrow = icon("Arrow", GRID_LARGE, "M5 19 19 5M10 5h9v9")
    val Rect =
        icon(
            "Rect",
            GRID_LARGE,
            "M6.5 6h11a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2h-11a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2Z",
        )
    val Ellipse = icon("Ellipse", GRID_LARGE, "M20 12a8 6 0 1 1-16 0a8 6 0 0 1 16 0Z")

    val ClearPage =
        icon(
            "ClearPage",
            GRID_MEDIUM,
            "M6 3.5h7l3.5 3.5v11.5H6Z",
            "M13 3.5V7h3.5",
            "M9 11.5l4 4M13 11.5l-4 4",
        )
    val Sync =
        icon(
            "Sync",
            GRID_MEDIUM,
            "M17 8a6.5 6.5 0 0 0-11.3-2.2L4 7.5",
            "M4 3.8v3.7h3.7",
            "M5 14a6.5 6.5 0 0 0 11.3 2.2L18 14.5",
            "M18 18.2v-3.7h-3.7",
        )
    val Home =
        icon(
            "Home",
            GRID_MEDIUM,
            "M3.8 10.5 11 4l7.2 6.5",
            "M5.7 9.2V18h10.6V9.2",
            "M9.2 18v-4.6h3.6V18",
        )
    val NewFrame =
        icon(
            "NewFrame",
            GRID_MEDIUM,
            "M4 4.5h14a1 1 0 0 1 1 1v11a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1v-11a1 1 0 0 1 1-1Z",
            "M3 8.5h16",
            "M11 11.2v4.6M8.7 13.5h4.6",
        )
    val Fit =
        icon(
            "Fit",
            GRID_MEDIUM,
            "M3.5 8V5.5a2 2 0 0 1 2-2H8",
            "M14 3.5h2.5a2 2 0 0 1 2 2V8",
            "M18.5 14v2.5a2 2 0 0 1-2 2H14",
            "M8 18.5H5.5a2 2 0 0 1-2-2V14",
        )

    private const val GRID_SMALL = 20f
    private const val GRID_MEDIUM = 22f
    private const val GRID_LARGE = 24f
    private const val STROKE = 1.6f
    private const val DASHES = 11
    private const val DASH_FRACTION = 0.5f

    /** An icon of [paths] (SVG path data, stroked) on a [grid]-unit square, plus whatever [extra] draws. */
    private fun icon(
        name: String,
        grid: Float,
        vararg paths: String,
        extra: ImageVector.Builder.() -> Unit = {},
    ): ImageVector {
        val builder = ImageVector.Builder(name, grid.dp, grid.dp, grid, grid)
        for (data in paths) builder.stroked(data)
        builder.extra()
        return builder.build()
    }

    private fun ImageVector.Builder.stroked(data: String) {
        addPath(
            pathData = PathParser().parsePathString(data).toNodes(),
            stroke = SolidColor(Color.Black),
            strokeLineWidth = STROKE,
            strokeLineCap = StrokeCap.Round,
            strokeLineJoin = StrokeJoin.Round,
        )
    }

    /** An ellipse drawn as [DASHES] short strokes, like the lasso's dotted outline (a vector has no dash pattern). */
    private fun ImageVector.Builder.dashedEllipse(
        cx: Float,
        cy: Float,
        rx: Float,
        ry: Float,
    ) {
        path(
            stroke = SolidColor(Color.Black),
            strokeLineWidth = STROKE,
            strokeLineCap = StrokeCap.Round,
            strokeLineJoin = StrokeJoin.Round,
        ) {
            val step = 2 * PI.toFloat() / DASHES
            for (i in 0 until DASHES) dash(cx, cy, rx, ry, i * step, i * step + step * DASH_FRACTION)
        }
    }

    private fun PathBuilder.dash(
        cx: Float,
        cy: Float,
        rx: Float,
        ry: Float,
        from: Float,
        to: Float,
    ) {
        moveTo(cx + rx * cos(from), cy + ry * sin(from))
        lineTo(cx + rx * cos(to), cy + ry * sin(to))
    }
}
