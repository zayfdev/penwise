package dev.penwise.frames

import androidx.core.view.isVisible

/**
 * One frame's page: a WebView at the preset's device size, fed by the bridge and by `FrameLayer.dispatch`'s
 * synthesised touches, never by the window's own dispatch (the ink surface is above it). At most two exist: the live
 * one and the one kept warm behind it.
 */
internal class FrameEntry(
    override var spec: FrameSpec,
    val view: FrameWebView,
    /** The view's device size and zoom for the spec (a preset switch replaces it). */
    var pixels: PagePixels,
) : FollowTarget {
    var placement = FramePlacement(0f, 0f, 0f, 0f, 1f)
    var scroll = FrameScroll.NONE

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

    /** Where the core puts the live window for the scroll the bridge last reported (before any layout: the box). */
    var base = WindowBase.at(spec.full)

    override val moves: Boolean get() = base.moves

    /** The WebView's own scroll offset in CSS px, as of its last `onScrollChanged`. */
    override var cssX = 0f
    override var cssY = 0f

    /** The window should still be made to cover the visible part (a fresh or woken page that has not been aligned). */
    override var needsAlign = false

    override val pxPerCss: Float get() = pixels.pxPerCss

    /** Set by [PageOps.resize] until the page's new zoom has taken ([FrameLayer] clears it). */
    override var reflowing = false

    /** The core's box height: the frame's document, measured or hinted, never less than the preset. */
    override val documentHeight: Float get() = spec.boxHeight.toFloat()

    /** The WebView is on screen (not parked warm). */
    val isShown: Boolean get() = view.isVisible

    override fun scrollToDevicePx(y: Int) = view.scrollTo(view.scrollX, y)

    override fun stopFling() = view.flingScroll(0, 0)

    /** The document's live height in CSS px: the box, the bridge's last report and the WebView's content height. */
    private val liveDocument: Float
        get() = ScrollLink.liveDocument(documentHeight, scroll.docHeight.toFloat(), view.contentHeight.toFloat())

    /** The live window on the board, from the page's actual scroll (held to the document's end) and the frame now. */
    fun window(): FrameRect = base.rect(spec, cssX, cssY, liveDocument)
}
