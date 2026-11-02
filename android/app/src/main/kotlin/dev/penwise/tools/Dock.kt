package dev.penwise.tools

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ColorFilter
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import dev.penwise.R
import dev.penwise.frames.ChromeStyle
import dev.penwise.sync.SyncStatus

private const val DOCK_HEIGHT_DP = 60
private const val DOCK_PAD_DP = 8
private const val DOCK_GAP_DP = 2
private const val HISTORY_WIDTH_DP = 44
private const val DELETE_WIDTH_DP = 40
private const val DIVIDER_HEIGHT_DP = 26
private const val DIVIDER_MARGIN_DP = 5
private const val WHEEL_ICON_DP = 26
private const val WHEEL_DOT_DP = 14
private const val WHEEL_DOT_RING_DP = 2
private const val ELEMENT_SP = 13
private const val OPAQUE = -0x1000000
private val HUES = listOf(0xFF5A4E, 0xFFB020, 0xF5E663, 0x4ADE80, 0x38BDF8, 0x6366F1, 0xD946EF, 0xFF5A4E)

/** What the dock's one-tap buttons do besides choosing tools. */
internal class DockCommands(
    val onUndo: () -> Unit,
    val onRedo: () -> Unit,
    val onSync: () -> Unit,
)

/** The sync link as the dock shows it. */
internal class SyncLink(
    val status: SyncStatus,
    val pending: Int,
)

/**
 * The compact floating dock (graphite): the pen, marker, eraser, shapes and select tools, the colour, the element
 * switch, undo and redo, the sync dot and the page actions. Pen, eraser, shapes, colour and more open flyouts above
 * their button ([flyouts], one at a time). [more] is the content of the page actions flyout. All the choices are
 * written to [state]; this only wires buttons to it. Its width never changes: the delete slot and the sync count
 * have reserved room, so no button moves under the finger.
 */
@Composable
internal fun Dock(
    state: ToolState,
    flyouts: DockFlyouts,
    commands: DockCommands,
    link: SyncLink,
    more: @Composable () -> Unit,
) {
    val colors = ChromeStyle.colors
    val shape = RoundedCornerShape(ChromeStyle.PANEL_RADIUS_DP.dp)
    Row(
        Modifier
            .height(DOCK_HEIGHT_DP.dp)
            .background(colors.panel, shape)
            .border(ChromeStyle.HAIRLINE_DP.dp, colors.hairline, shape)
            .padding(horizontal = DOCK_PAD_DP.dp),
        horizontalArrangement = Arrangement.spacedBy(DOCK_GAP_DP.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        DrawingTools(state, flyouts)
        Divider()
        ColourButton(state, flyouts)
        ElementButton(state)
        Divider()
        HistoryButtons(commands)
        SyncDot(link.status, link.pending, commands.onSync)
        MoreButton(flyouts, more)
    }
}

@Composable
private fun ElementButton(state: ToolState) {
    val colors = ChromeStyle.colors
    val label = stringResource(if (state.bindToElement) R.string.frame_bind_element else R.string.frame_bind_area)
    DockSlot(label, state.bindToElement, state::toggleBindTarget, width = null) {
        val tint = if (state.bindToElement) colors.iconOn else colors.icon
        Row(
            Modifier.padding(horizontal = 8.dp),
            horizontalArrangement = Arrangement.spacedBy(8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Image(
                DockIcons.Element,
                null,
                Modifier.size(DockIcons.Element.defaultWidth),
                colorFilter = ColorFilter.tint(tint),
            )
            DockText(label, tint, ELEMENT_SP)
        }
    }
}

@Composable
private fun HistoryButtons(commands: DockCommands) {
    DockButton(DockIcons.Undo, stringResource(R.string.tool_undo), false, commands.onUndo, width = HISTORY_WIDTH_DP.dp)
    DockButton(DockIcons.Redo, stringResource(R.string.tool_redo), false, commands.onRedo, width = HISTORY_WIDTH_DP.dp)
}

@Composable
private fun MoreButton(
    flyouts: DockFlyouts,
    more: @Composable () -> Unit,
) {
    val open = flyouts.isOpen(DockFlyout.MORE)
    Box {
        DockButton(
            DockIcons.More,
            stringResource(if (open) R.string.tool_tools_close else R.string.tool_tools_open),
            open,
            { flyouts.toggle(DockFlyout.MORE) },
            width = HISTORY_WIDTH_DP.dp,
        )
        Flyout(open) { more() }
    }
}

/** Pen, marker, eraser, shapes, select, and the delete slot (kept in place, shown while something is selected). */
@Composable
private fun DrawingTools(
    state: ToolState,
    flyouts: DockFlyouts,
) {
    Box {
        val inUse = state.drawing && !state.markerSelected
        val open = flyouts.isOpen(DockFlyout.PEN)
        DockButton(DockIcons.Pen, stringResource(R.string.tool_pen), inUse || open, {
            tap(DockTaps.pen(inUse, open), flyouts, DockFlyout.PEN, state::selectPen)
        })
        Flyout(open) { PenFlyout(state) }
    }
    val marker = state.drawing && state.markerSelected
    DockButton(DockIcons.Marker, stringResource(R.string.tool_marker), marker, {
        tap(DockTaps.marker(marker), flyouts, DockFlyout.PEN, state::selectMarker)
    })
    Box {
        DockButton(
            DockIcons.Eraser,
            stringResource(R.string.tool_eraser),
            state.eraserMode,
            {
                val turnOn = DockTaps.eraser(state.eraserMode) == ToolTap.ACTIVATE
                tap(DockTaps.eraser(state.eraserMode), flyouts, DockFlyout.ERASER) { if (turnOn) state.toggleEraser() }
            },
        )
        Flyout(flyouts.isOpen(DockFlyout.ERASER)) { EraserFlyout(state) }
    }
    Box {
        DockButton(
            DockIcons.Shapes,
            stringResource(R.string.drawer_shapes),
            state.shapeMode != null || flyouts.isOpen(DockFlyout.SHAPES),
            { flyouts.toggle(DockFlyout.SHAPES) },
        )
        Flyout(flyouts.isOpen(DockFlyout.SHAPES)) { ShapesFlyout(state, onPicked = flyouts::close) }
    }
    DockButton(DockIcons.Select, stringResource(R.string.tool_lasso), state.lassoMode, {
        flyouts.close()
        state.toggleLasso()
    })
    DeleteSlot(state)
}

/** The delete-selection button in a slot that is always there, so the dock keeps its width. */
@Composable
private fun DeleteSlot(state: ToolState) {
    Box(Modifier.width(DELETE_WIDTH_DP.dp), contentAlignment = Alignment.Center) {
        AnimatedVisibility(
            visible = state.selection.count > 0,
            enter = fadeIn(tween(DockMotion.SELECT_MS, easing = DockMotion.easeOut)),
            exit = fadeOut(tween(DockMotion.SELECT_MS, easing = DockMotion.easeOut)),
        ) {
            DockButton(
                DockIcons.Delete,
                stringResource(R.string.tool_delete_selection),
                false,
                state.selection::requestDelete,
                width = DELETE_WIDTH_DP.dp,
            )
        }
    }
}

/** Runs a tool button's tap: [ToolTap.ACTIVATE] closes the flyouts and runs [activate]; the other toggles [flyout]. */
private fun tap(
    decision: ToolTap,
    flyouts: DockFlyouts,
    flyout: DockFlyout,
    activate: () -> Unit,
) {
    if (decision == ToolTap.TOGGLE_FLYOUT) {
        flyouts.toggle(flyout)
    } else {
        flyouts.close()
        activate()
    }
}

/** The colour button: the hue wheel in miniature with the pen's colour in its middle; opens the colour flyout. */
@Composable
private fun ColourButton(
    state: ToolState,
    flyouts: DockFlyouts,
) {
    val colors = ChromeStyle.colors
    val open = flyouts.isOpen(DockFlyout.COLOUR)
    Box {
        DockSlot(stringResource(R.string.tool_colour), open, { flyouts.toggle(DockFlyout.COLOUR) }) {
            val pen = Color(state.argb() or OPAQUE)
            Canvas(Modifier.size(WHEEL_ICON_DP.dp)) {
                drawCircle(
                    Brush.sweepGradient(HUES.map { Color(it or OPAQUE) }, Offset(size.width / 2, size.height / 2)),
                )
            }
            Box(
                Modifier
                    .size(WHEEL_DOT_DP.dp)
                    .background(pen, CircleShape)
                    .border(WHEEL_DOT_RING_DP.dp, if (open) colors.buttonSelected else colors.panel, CircleShape),
            )
        }
        Flyout(open) { ColourFlyout(state) }
    }
}

@Composable
private fun Divider() {
    Box(
        Modifier
            .padding(horizontal = DIVIDER_MARGIN_DP.dp)
            .width(1.dp)
            .height(DIVIDER_HEIGHT_DP.dp)
            .background(ChromeStyle.colors.divider),
    )
}
