package dev.penwise.overview

import dev.penwise.frames.FrameServer
import dev.penwise.frames.FrameSpec
import dev.penwise.frames.LaneKey
import dev.penwise.frames.SwitcherItem
import dev.penwise.frames.SwitcherState
import dev.penwise.frames.lane
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update

/** The read-only viewer of one past picture: [shown] is the picture's board item. */
internal data class HistoryView(
    val lane: LaneKey,
    val generation: Generation,
    val shown: String,
)

/**
 * What the overview shows now. The board's frames are kept here ([specs] of [boardPage]) because a sketch page opened
 * from the overview has no frames of its own: its overview button reopens the overview from this copy.
 */
internal data class OverviewUi(
    val open: Boolean = false,
    val boardPage: String? = null,
    val specs: List<FrameSpec> = emptyList(),
    val viewer: HistoryView? = null,
    /** The sketch page that was opened from the overview, or null. */
    val sketch: String? = null,
    /** The screen the owner was on last (the locked frame's, a viewed picture's or a sketch's): its strip opens. */
    val lane: LaneKey? = null,
)

/** The overview's state machine (contract §3.3). Plain state on a flow; the UI thread drives it. */
internal class OverviewController {
    private val state = MutableStateFlow(OverviewUi())
    private val attachedTicks = MutableStateFlow(0)

    val ui: StateFlow<OverviewUi> = state.asStateFlow()

    /** Counts sketch pages that reached their strip on the server: an open overview fetches the strips again. */
    val attached: StateFlow<Int> = attachedTicks.asStateFlow()

    /** Sends new sketch pages to the server (set once by the view model); null in tests that do not need it. */
    var sketches: SketchAttacher? = null
        private set

    /** Wires the attach of new sketch pages to [server] ([outbox]: ops still waiting for the server's ack). */
    fun attachSketches(
        scope: CoroutineScope,
        server: () -> FrameServer?,
        outbox: () -> Int,
    ) {
        val send = { lane: LaneKey, page: String ->
            server()?.let { StripPagesClient.attach(it, lane, page) } ?: AttachResult.FAILED
        }
        sketches = SketchAttacher(scope, send, outbox, onAttached = { attachedTicks.update { it + 1 } })
    }

    /**
     * The overview was asked for (a pinch, the button, the agent) while [page] showed [specs] with [locked] locked. A
     * page without frames opens it from the board seen last; with no board seen yet there is nothing to show.
     */
    fun request(
        page: String,
        specs: List<FrameSpec>,
        locked: String? = null,
    ) {
        state.update { now ->
            when {
                specs.isNotEmpty() -> {
                    val lane = specs.firstOrNull { it.id == locked }?.lane ?: now.lane
                    now.copy(open = true, boardPage = page, specs = specs, viewer = null, lane = lane)
                }

                now.boardPage != null -> {
                    now.copy(open = true, viewer = null)
                }

                else -> {
                    now
                }
            }
        }
    }

    /** The open board page's frames changed: the copy follows, so the strips refetch after a round resolved. */
    fun boardFrames(
        page: String,
        specs: List<FrameSpec>,
    ) {
        if (specs.isEmpty()) return
        state.update {
            if (it.boardPage == page ||
                it.boardPage == null
            ) {
                it.copy(boardPage = page, specs = specs)
            } else {
                it
            }
        }
    }

    fun close() = state.update { it.copy(open = false) }

    /** A sketch card was tapped: that page opens as a free canvas and the overview button leads back. */
    fun enterSketch(
        page: String,
        lane: LaneKey,
    ) = state.update { it.copy(open = false, sketch = page, lane = lane) }

    /** A past card was tapped: the viewer opens on [item] of [generation]. */
    fun view(
        lane: LaneKey,
        generation: Generation,
        item: String,
    ) = state.update { it.copy(open = false, viewer = HistoryView(lane, generation, item), lane = lane) }

    /** The viewer's switcher moved to another picture of the generation. */
    fun show(item: String) = state.update { now -> now.copy(viewer = now.viewer?.copy(shown = item)) }

    /** Back from the viewer to the overview. */
    fun leaveViewer() = state.update { if (it.viewer == null) it else it.copy(open = true, viewer = null) }

    /** The viewer goes away for a frame that was entered ("Back to now"). */
    fun dropViewer() = state.update { it.copy(viewer = null) }

    /** The overview button shows on a board, and on the sketch page [active] that was opened from the overview. */
    fun showsButton(
        active: String,
        hasFrames: Boolean,
    ): Boolean = hasFrames || (state.value.sketch == active && state.value.boardPage != null)
}

/** The switcher row of a past generation: OG, then the variants by order, a check on the pick; no "Use this". */
internal object HistorySwitcher {
    fun of(view: HistoryView): SwitcherState {
        val generation = view.generation
        val items =
            listOf(SwitcherItem(generation.og.item, SwitcherState.OG_LABEL, og = true, chosen = false)) +
                generation.variants.map {
                    SwitcherItem(it.picture.item, it.label.ifEmpty { "?" }, og = false, chosen = it.picked)
                }
        return SwitcherState(items, view.shown, pickable = false)
    }

    /** The picture of [view] that is shown (the OG when the item is unknown). */
    fun picture(view: HistoryView): HistoryPicture =
        view.generation.variants
            .map { it.picture }
            .firstOrNull { it.item == view.shown } ?: view.generation.og
}
