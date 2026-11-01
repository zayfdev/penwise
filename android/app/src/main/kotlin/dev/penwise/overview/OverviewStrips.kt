package dev.penwise.overview

import dev.penwise.frames.FrameLane
import dev.penwise.frames.FrameSpec
import dev.penwise.frames.LaneKey
import dev.penwise.frames.lane

/** One column of a strip (contract §3.3), left to right: past generations, now, sketch pages, then `+`. */
internal sealed interface OverviewColumn {
    /** Stable across refetches, so the strip keeps the owner on the same column when the list changes. */
    val key: String

    /** A resolved round: its OG picture with the variants under it. */
    data class Past(
        val generation: Generation,
    ) : OverviewColumn {
        override val key: String get() = "past-${generation.round}"
    }

    /** The lane's live frame (OG) with its live variants under it. */
    data class Now(
        val og: FrameSpec,
        val variants: List<FrameSpec>,
        /** The round the frames are in: the project's open round on the server. */
        val round: Int,
    ) : OverviewColumn {
        override val key: String get() = NOW_KEY
    }

    /** A sketch page attached to the strip. */
    data class Sketch(
        val page: String,
        val title: String,
    ) : OverviewColumn {
        override val key: String get() = "sketch-$page"
    }

    /** The end of the strip: add a sketch page. */
    data object Plus : OverviewColumn {
        override val key: String get() = "plus"
    }

    companion object {
        const val NOW_KEY = "now"
    }
}

/** One screen (route + preset) of the project's board and its timeline. */
internal data class OverviewStrip(
    val lane: LaneKey,
    val title: String,
    val live: FrameSpec,
    val columns: List<OverviewColumn>,
) {
    /** Where the "now" column sits: the strip opens centred on it. */
    val nowIndex: Int get() = columns.indexOfFirst { it.key == OverviewColumn.NOW_KEY }.coerceAtLeast(0)
}

/** Builds the strips of the overview from the board's frames and what the server keeps. Pure. */
internal object OverviewStrips {
    /**
     * One strip per lane that has a live frame, top lane first (lanes stack downward on the board). Each strip lists
     * its past generations oldest first (from [history], newest first as sent), the now column, the sketch pages of
     * [stripPages] for which [pageTitle] knows a title (a page deleted from the library has none), and `+`. The now
     * column carries the project's open round from [openRounds]; without it, one after the newest resolved one.
     */
    fun of(
        specs: List<FrameSpec>,
        history: Map<LaneKey, List<Generation>>,
        stripPages: Map<LaneKey, List<StripPage>>,
        openRounds: Map<String, Int> = emptyMap(),
        pageTitle: (String) -> String?,
    ): List<OverviewStrip> =
        lives(specs).map { live ->
            val lane = live.lane
            val past = history[lane].orEmpty().sortedBy { it.round }
            val sketches =
                stripPages[lane].orEmpty().mapNotNull { attached ->
                    pageTitle(attached.page)?.let { OverviewColumn.Sketch(attached.page, it) }
                }
            val round = openRounds[lane.project] ?: ((past.lastOrNull()?.round ?: 0) + 1)
            val now = OverviewColumn.Now(live, FrameLane.variants(specs, lane), round)
            OverviewStrip(
                lane = lane,
                title = title(lane),
                live = live,
                columns = past.map { OverviewColumn.Past(it) } + now + sketches + OverviewColumn.Plus,
            )
        }

    /** The strip of [lane]; 0 when there is none (the lane's frames are gone). */
    fun indexOf(
        strips: List<OverviewStrip>,
        lane: LaneKey?,
    ): Int = strips.indexOfFirst { it.lane == lane }.coerceAtLeast(0)

    /**
     * Changes when a lane gets a new live frame or a new variant group (a round resolved): the overview refetches the
     * history then. Frame positions are part of it, since a resolve moves the live frame right.
     */
    fun generationKey(specs: List<FrameSpec>): String =
        specs.joinToString("|") { "${it.id}@${it.x.toInt()}:${it.group.orEmpty()}" }

    /** "Desktop /" : the preset and the route of the screen. */
    fun title(lane: LaneKey): String = lane.preset.replaceFirstChar { it.uppercase() } + " " + lane.route

    private fun lives(specs: List<FrameSpec>): List<FrameSpec> =
        specs
            .map { it.lane }
            .distinct()
            .mapNotNull { FrameLane.live(specs, it) }
            .sortedWith(compareBy({ it.y }, { it.x }))
}
