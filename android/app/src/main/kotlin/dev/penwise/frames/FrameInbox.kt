package dev.penwise.frames

import android.util.Log

/** What the bridge message handling asks of the layer. */
internal interface InboxHost {
    /** Only the live page's scroll and layout reports reach the core. */
    fun isLive(entry: FrameEntry): Boolean

    fun scrolled(
        entry: FrameEntry,
        scroll: FrameScroll,
    )

    fun layout(
        entry: FrameEntry,
        message: FrameMessage.Layout,
        json: String,
    )

    fun hello(entry: FrameEntry)

    fun answered(
        id: Int,
        key: String,
        rect: List<Float>,
    )

    /** Runs [commit] after [delayMs], replacing a commit still waiting. */
    fun reschedule(
        commit: Runnable,
        delayMs: Long,
    )

    fun committed(
        entry: FrameEntry,
        route: String,
    )
}

/**
 * What a page's bridge message does to its [FrameEntry]: the scroll offset kept for the ink layer (M2's area marks),
 * the layout handed to the core, answers to hit requests, and the page's own navigation, which updates its item's route
 * once the clicking stops.
 */
internal object FrameInbox {
    fun handle(
        entry: FrameEntry,
        message: FrameMessage,
        host: InboxHost,
    ) {
        val id = entry.spec.id
        when (message) {
            is FrameMessage.Hello -> {
                // The document size is known before the first scroll: the readout and M2's area marks want it now.
                entry.scroll = FrameScroll(entry.scroll.x, entry.scroll.y, message.docWidth, message.docHeight)
                FrameRoutes.ofUrl(message.url)?.let { entry.reportedRoute = it }
                if (host.isLive(entry)) host.scrolled(entry, entry.scroll)
                host.hello(entry)
                Log.i(
                    TAG,
                    "$id hello ${message.url} doc ${message.docWidth}x${message.docHeight} " +
                        "view ${message.viewWidth}x${message.viewHeight}",
                )
            }

            is FrameMessage.Scroll -> {
                entry.scroll = FrameScroll(message.x, message.y, message.docWidth, message.docHeight)
                if (host.isLive(entry)) host.scrolled(entry, entry.scroll)
            }

            is FrameMessage.Route -> {
                queueRoute(entry, message.route, host)
            }

            is FrameMessage.Hit -> {
                host.answered(message.id, message.key, message.rect)
                Log.d(TAG, "$id hit ${message.key} '${message.selector}' ${message.rect}")
            }

            is FrameMessage.Layout -> {
                if (host.isLive(entry)) host.layout(entry, message, FrameLayouts.coreJson(message, entry.spec.preset))
            }

            is FrameMessage.Replayed -> {
                Log.i(TAG, "$id replay '${message.state}' ok=${message.ok}")
            }

            is FrameMessage.Patched -> {
                Log.i(TAG, "$id patched ok=${message.ok} failed=${message.failed}")
            }
        }
    }

    /**
     * The page navigated by itself. Its item follows when the user stops: a menu that pushes several routes while it
     * settles must not queue an op each, and the page is already there, so the item update reloads nothing.
     */
    private fun queueRoute(
        entry: FrameEntry,
        route: String,
        host: InboxHost,
    ) {
        val normalized = FrameRoutes.normalize(route) ?: return
        // Already known (a repeated report of the same navigation) or ignored: nothing to tell the view model.
        if (android.os.SystemClock.uptimeMillis() < entry.quietUntil) return
        if (normalized == entry.reportedRoute || normalized == entry.spec.route) return
        entry.reportedRoute = normalized
        entry.pendingRoute = normalized
        val runnable =
            entry.routeRunnable
                ?: Runnable {
                    val pending = entry.pendingRoute ?: return@Runnable
                    entry.pendingRoute = null
                    host.committed(entry, pending)
                }.also { entry.routeRunnable = it }
        host.reschedule(runnable, ROUTE_DEBOUNCE_MS)
    }

    private const val TAG = "PenwiseFrames"

    /** How long a route report is held so a flurry of link clicks becomes one item update. */
    private const val ROUTE_DEBOUNCE_MS = 500L
}
