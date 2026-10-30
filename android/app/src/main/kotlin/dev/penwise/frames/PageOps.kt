package dev.penwise.frames

import android.os.SystemClock
import android.widget.FrameLayout

/** What is done to a frame's WebView once it exists: load, reflow, reload, and what the bridge is told. */
internal object PageOps {
    /** Route reports right after a programmatic load describe the document being replaced. */
    private const val LOAD_QUIET_MS = 1_500L

    /**
     * Points [entry] at [spec]: a page that already shows the spec's route (the user clicked a link; the item follows)
     * adopts it without a reload, anything else (another project, another host, a route typed by the user) loads.
     */
    fun load(
        entry: FrameEntry,
        spec: FrameSpec,
    ) {
        val adopted =
            entry.reportedRoute == spec.route && FrameUrls.origin(entry.loadedUrl) == FrameUrls.origin(spec.url)
        entry.spec = spec
        if (adopted) {
            entry.loadedUrl = spec.url
            return
        }
        entry.loadedUrl = spec.url
        entry.reportedRoute = spec.route
        entry.quietUntil = SystemClock.uptimeMillis() + LOAD_QUIET_MS
        entry.view.loadUrl(spec.url)
    }

    /** A preset switch: the page keeps its document and reflows at the new width (its breakpoints see the preset). */
    fun resize(
        entry: FrameEntry,
        spec: FrameSpec,
        density: Float,
    ) {
        entry.spec = spec
        entry.view.layoutParams =
            FrameLayout.LayoutParams((spec.cssWidth * density).toInt(), (spec.cssHeight * density).toInt())
    }

    /** Loads one frame's page again (the chip's reload); its scroll position comes back through the bridge. */
    fun reload(entry: FrameEntry) {
        entry.reportedRoute = entry.spec.route
        entry.quietUntil = SystemClock.uptimeMillis() + LOAD_QUIET_MS
        entry.view.reload()
    }

    /** Tells the page which element keys the marks need watched. */
    fun sendWatch(
        entry: FrameEntry,
        keys: List<String>,
    ) {
        FrameCommands.watch(entry.view, keys)
    }

    /** Sends the patch decision the page does not have yet; a silent no-op when it already shows that. */
    fun sendPatch(entry: FrameEntry) {
        val message = entry.patches.next(entry.spec.patch, entry.patchOn) ?: return
        FrameCommands.patch(entry.view, message)
    }
}
