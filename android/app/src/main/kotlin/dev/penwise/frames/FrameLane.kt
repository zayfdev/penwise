package dev.penwise.frames

import dev.penwise.ink.PageRect

/** A lane: the frames of one route at one preset in one project. */
internal data class LaneKey(
    val project: String,
    val route: String,
    val preset: String,
)

internal val FrameSpec.lane: LaneKey get() = LaneKey(project, route, preset)

/** The frame's box on the board: what the camera is locked to. */
internal val FrameSpec.box: PageRect get() = PageRect(x, y, cssWidth.toFloat(), boxHeight.toFloat())

/**
 * Who is who in a lane (locked-frames contract §3.2). The live frame is the rightmost plain frame of the lane, the
 * way the server picks it (`find_frame` in `agent/board.rs`: rounds move it right); its variants are the grouped
 * frames of the same lane, in label order.
 */
internal object FrameLane {
    /** The lane's live frame ("OG"), or null when the lane has only variants. */
    fun live(
        specs: List<FrameSpec>,
        lane: LaneKey,
    ): FrameSpec? =
        specs
            .filter { it.group == null && it.lane == lane }
            .fold(null as FrameSpec?) { best, spec -> if (best == null || spec.x > best.x) spec else best }

    /**
     * The lane's variants by label (then by position, so equal labels keep the board's order). Only the newest group
     * counts: the specs come in paint order (z ascending) and a later round's frames are stacked above an earlier
     * one's, so the group of the last grouped frame of the lane is the newest. Group ids are random, not ordered.
     */
    fun variants(
        specs: List<FrameSpec>,
        lane: LaneKey,
    ): List<FrameSpec> {
        val grouped = specs.filter { it.group != null && it.lane == lane }
        val newest = grouped.lastOrNull()?.group ?: return emptyList()
        return grouped.filter { it.group == newest }.sortedWith(compareBy<FrameSpec>({ it.label }, { it.x }))
    }
}
