package dev.penwise.frames

/** Where a layout the core is given comes from. */
enum class LayoutKind {
    /** The live page's bridge report: it merges into the frame's layout and moves the live window. */
    LIVE,

    /** A still's layout for a frame that is not live: it merges, like any report. */
    STILL,

    /** The frame just stopped being live: its still's layout replaces the live one, nothing of it is kept. */
    DEMOTED,
}
