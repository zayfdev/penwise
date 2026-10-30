package dev.penwise.frames

import android.os.SystemClock
import dev.penwise.ink.FitMode
import dev.penwise.ink.PageRect
import dev.penwise.ink.PenActivity
import dev.penwise.ink.ViewportState
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

/** Reaches the frame layer from outside the composition (the layer exists only while the page has frames). */
internal class FrameControl {
    @Volatile
    var layer: FrameLayer? = null
}

/** Where the open page is and how to change it, for [FrameAgent]. */
internal class AgentPage(
    val current: () -> String,
    val open: (String) -> Unit,
    val title: (String) -> String,
    val specs: () -> List<FrameSpec>,
    /** The core's bounds of everything on the open page (all layers), or null when it is empty. */
    val contentBounds: () -> PageRect?,
)

/**
 * The camera moves other devices ask for (`SwitchPage`, `Focus`; contract §3): the page changes, then the item is fit
 * to the screen; an item that is unknown or no frame means the page's content. While the owner is drawing (the last
 * pen stroke is under 10 s old) nothing moves: [chip] offers the move instead, and a tap on it does it.
 * All on the UI thread.
 */
internal class FrameAgent(
    private val scope: CoroutineScope,
    private val viewport: ViewportState,
    private val pen: PenActivity,
    private val control: FrameControl,
    private val page: AgentPage,
    private val now: () -> Long = SystemClock::uptimeMillis,
) {
    private val moves = AgentMoves()
    private var waiting: Waiting? = null
    private var timeout: Job? = null

    private class Waiting(
        val page: String,
        val item: String?,
    )

    private val _chip = MutableStateFlow<MoveTarget?>(null)

    /** The held move behind "Agent: open <label> →", or null. */
    val chip: StateFlow<MoveTarget?> = _chip.asStateFlow()

    /** Another device asked for the view to move; [target.label] is filled here. */
    fun offer(target: MoveTarget) {
        val labelled = target.copy(label = label(target))
        if (!moves.admit(labelled, now(), pen.lastDrawAt)) {
            _chip.value = moves.pending
            return
        }
        _chip.value = null
        apply(labelled)
    }

    /** The chip was tapped. */
    fun accept() {
        _chip.value = null
        moves.take()?.let(::apply)
    }

    /** The chip was dismissed. */
    fun dismiss() {
        moves.dismiss()
        _chip.value = null
    }

    /** The open page's frames changed (a sweep finished): a fit that waited for them can go on. */
    fun framesChanged() {
        val wanted = waiting ?: return
        if (page.current() != wanted.page) return
        if (wanted.item == null || page.specs().any { it.id == wanted.item }) finish(wanted)
    }

    private fun apply(target: MoveTarget) {
        val onPage = page.current() == target.page
        if (!onPage) page.open(target.page)
        if (!target.fit) return
        val wanted = Waiting(target.page, target.item)
        waiting = wanted
        if (onPage) {
            finish(wanted)
            return
        }
        // The new page's frames arrive with its first sweeps; if the item never shows up, fit the content anyway.
        timeout?.cancel()
        timeout =
            scope.launch {
                delay(FALLBACK_MS)
                if (waiting === wanted) finish(wanted)
            }
    }

    private fun finish(wanted: Waiting) {
        waiting = null
        timeout?.cancel()
        val list = page.specs()
        val spec = wanted.item?.let { id -> list.firstOrNull { it.id == id } }
        when {
            spec != null -> control.layer?.fitFrame(spec.id) ?: viewport.fitRect(boxOf(spec), FitMode.FIT_WIDTH)
            else -> page.contentBounds()?.let { viewport.fitRect(it, FitMode.FIT_ALL) } ?: viewport.reset()
        }
    }

    private fun label(target: MoveTarget): String {
        val spec = target.item?.let { id -> page.specs().firstOrNull { it.id == id } }
        return when {
            spec != null -> spec.label.ifEmpty { spec.route }
            else -> page.title(target.page)
        }
    }

    private fun boxOf(spec: FrameSpec) = PageRect(spec.x, spec.y, spec.cssWidth.toFloat(), spec.boxHeight.toFloat())

    private companion object {
        const val FALLBACK_MS = 2_500L
    }
}
