package dev.penwise.frames

import android.webkit.WebView

/**
 * One frame's page. A frame's window view: a WebView at the preset's device size, fed by the bridge and by
 * `FrameLayer.dispatch`'s synthesised touches, never by the window's own dispatch (the ink surface is above it).
 */
internal class FrameEntry(
    var spec: FrameSpec,
    val view: WebView,
) {
    var placement = FramePlacement(0f, 0f, 0f, 0f, 1f)
    var scroll = FrameScroll.NONE

    /** The last `pw:layout`; a pen-down merges its hit rect into it before committing a bound stroke. */
    var layout: FrameMessage.Layout? = null

    /** The URL this WebView was last told to load; what [FrameUrls.origin] compares before a reload. */
    var loadedUrl: String = spec.url

    /** The route the page itself last reported (or was sent to): a spec that matches it needs no reload. */
    var reportedRoute: String = spec.route

    /** Programmatic loads ignore the route messages of the document they replace. */
    var quietUntil: Long = 0L

    var pendingRoute: String? = null
    var routeRunnable: Runnable? = null

    /** The `pw:patch` decisions already sent to this page; reset on every load. */
    val patches = PatchFeed()

    /** The owner's local Ghost toggle: a before/after view, never synced; gone with the frame. */
    var patchOn = true
}
