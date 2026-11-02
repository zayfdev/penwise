package dev.penwise.tools

/** What a tap on a tool button does. */
internal enum class ToolTap {
    /** Switch to the tool (and close any flyout). */
    ACTIVATE,

    /** Open or close the tool's flyout; the tool keeps its choice. */
    TOGGLE_FLYOUT,
}

/**
 * The dock's tap decisions, plain functions so they are unit-tested. A tool that is already in use opens its flyout on
 * the next tap; while that flyout is open the button only closes it, whatever brush was picked inside.
 */
internal object DockTaps {
    /** The Pen button: [inUse] when the pen writes with a brush that is not the marker. */
    fun pen(
        inUse: Boolean,
        flyoutOpen: Boolean,
    ): ToolTap = if (inUse || flyoutOpen) ToolTap.TOGGLE_FLYOUT else ToolTap.ACTIVATE

    /** The Marker button: its flyout is the pen flyout (the marker is one of its brushes). */
    fun marker(inUse: Boolean): ToolTap = if (inUse) ToolTap.TOGGLE_FLYOUT else ToolTap.ACTIVATE

    /** The Eraser button: the first tap turns the eraser on, the next opens its flyout (precise, size). */
    fun eraser(eraserOn: Boolean): ToolTap = if (eraserOn) ToolTap.TOGGLE_FLYOUT else ToolTap.ACTIVATE
}
