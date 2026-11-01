package dev.penwise.frames

import android.content.Context
import android.view.View
import android.widget.FrameLayout

/**
 * The WebViews of the frames: at most one live page and the one kept warm behind it (see [PageSlots]). Opening,
 * parking, waking and destroying a WebView are here; where the live page sits and who is focused are the layer's
 * business.
 */
internal class LivePages(
    private val parent: FrameLayout,
    private val context: Context,
    private val sink: (frameId: String, message: FrameMessage) -> Unit,
    private val onScroll: ScrollListenerFor,
    private val onRelease: (FrameEntry) -> Unit,
) {
    private val slots =
        PageSlots<FrameEntry>({ it.spec.id }, ::open, ::wake, ::park, ::destroy)

    /** The page the owner works in. */
    val live: FrameEntry? get() = slots.live

    fun entry(id: String): FrameEntry? = slots.entry(id)

    fun all(): List<FrameEntry> = slots.all()

    /** Makes [spec] the live page (null: none); returns the page that stopped being live. */
    fun focus(spec: FrameSpec?): FrameEntry? = slots.focus(spec)

    /** A frame is gone: its page is destroyed whether it was live or warm. */
    fun close(id: String) = slots.close(id)

    /** Applies the diff of two frame lists to the pages that exist: a changed URL loads, a changed size re-lays out. */
    fun update(
        plan: FramePlan,
        next: List<FrameSpec>,
    ) {
        for (id in plan.close) close(id)
        for (spec in plan.load) entry(spec.id)?.let { PageOps.load(it, spec) }
        val metrics = context.resources.displayMetrics
        for (spec in plan.resize) entry(spec.id)?.let { PageOps.resize(it, spec, PagePixels.on(spec, metrics)) }
        for (spec in next) entry(spec.id)?.spec = spec
    }

    /** Destroys every page (page leave). */
    fun destroyAll() = slots.destroyAll()

    /** False while the app is in the background: pages opened or woken then stay paused. */
    var running = true
        private set

    /**
     * The app went to the background or came back: the live page stops its timers, or runs again. `pauseTimers` is
     * process-wide, which is fine here because the frames' WebViews are the only ones in the process.
     */
    fun setRunning(active: Boolean) {
        running = active
        val view = live?.view ?: return
        if (active) {
            view.resumeTimers()
            view.onResume()
        } else {
            view.onPause()
            view.pauseTimers()
        }
    }

    private fun open(spec: FrameSpec): FrameEntry {
        val pixels = PagePixels.on(spec, context.resources.displayMetrics)
        val view = FrameViews.open(context, spec, pixels) { message -> sink(spec.id, message) }
        val entry = FrameEntry(spec, view, pixels)
        view.onScroll = ScrollListener { x, y -> onScroll.scrolled(entry, x, y) }
        parent.addView(view)
        entry.needsAlign = true
        // Opened while the app is in the background (an agent's Focus): it loads but does not run.
        if (!running) {
            view.onPause()
            view.pauseTimers()
        }
        return entry
    }

    private fun park(entry: FrameEntry) {
        onRelease(entry)
        entry.view.onPause()
        entry.view.visibility = View.INVISIBLE
    }

    private fun wake(entry: FrameEntry) {
        entry.view.visibility = View.VISIBLE
        if (running) entry.view.onResume()
        entry.needsAlign = true
    }

    /** Takes one page's WebView down; its content is gone, so a frame that comes back loads again. */
    private fun destroy(entry: FrameEntry) {
        // A frame deleted under the finger: the page gets a cancel, never a click into a destroyed view.
        onRelease(entry)
        entry.routeRunnable?.let(parent::removeCallbacks)
        entry.routeRunnable = null
        entry.pendingRoute = null
        entry.view.onScroll = null
        parent.removeView(entry.view)
        entry.view.destroy()
    }
}

/** The page of a frame scrolled to ([x], [y]) device px. */
internal fun interface ScrollListenerFor {
    fun scrolled(
        entry: FrameEntry,
        x: Int,
        y: Int,
    )
}
