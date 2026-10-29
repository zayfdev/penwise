package dev.penwise.tools

import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.Redo
import androidx.compose.material.icons.automirrored.filled.Undo
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.ContentCut
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.DeleteSweep
import androidx.compose.material.icons.filled.Edit
import androidx.compose.material.icons.filled.FitScreen
import androidx.compose.material.icons.filled.Home
import androidx.compose.material.icons.filled.HorizontalRule
import androidx.compose.material.icons.filled.Language
import androidx.compose.material.icons.filled.NorthEast
import androidx.compose.material.icons.filled.Sync
import androidx.compose.material.icons.outlined.Circle
import androidx.compose.material.icons.outlined.HighlightAlt
import androidx.compose.material.icons.outlined.Rectangle

/** The icon set: standard Material glyphs, so every tool reads the same way as in other apps. */
object ToolIcons {
    val Eraser = EraserIcon
    val PreciseEraser = Icons.Filled.ContentCut
    val Undo = Icons.AutoMirrored.Filled.Undo
    val Redo = Icons.AutoMirrored.Filled.Redo
    val ClearPage = Icons.Filled.DeleteSweep
    val Delete = Icons.Filled.Delete
    val Sync = Icons.Filled.Sync
    val Home = Icons.Filled.Home
    val Frame = Icons.Filled.Language
    val Fit = Icons.Filled.FitScreen
    val Add = Icons.Filled.Add
    val Rename = Icons.Filled.Edit
    val Line = Icons.Filled.HorizontalRule
    val Rect = Icons.Outlined.Rectangle
    val Ellipse = Icons.Outlined.Circle
    val Arrow = Icons.Filled.NorthEast
    val Lasso = Icons.Outlined.HighlightAlt
}

private const val LINE_ANGLE = -45f

/** Rotation that turns the horizontal-rule icon into a diagonal line, distinct from the arrow. */
const val LINE_ICON_ROTATION = LINE_ANGLE
