package dev.penwise.frames

/** Where a layout the core is given comes from; what the core does with it (contract §7). */
enum class LayoutKind(
    /** The layout replaces whatever the frame had instead of merging into it. */
    val replaces: Boolean,
) {
    /** The live page's bridge report: it merges into the frame's layout and moves the live window. */
    LIVE(replaces = false),

    /** A new still for a frame that is not live: its layout replaces the frame's, nothing of the old one is kept. */
    STILL(replaces = true),

    /** The frame just stopped being live and has a still: the still's layout replaces the live one. */
    DEMOTED(replaces = true),
}
