package dev.penwise.frames

import android.content.Context
import android.webkit.WebView

/**
 * A frame's page view, which tells its host the moment its own scroll offset changes (device px), in the same UI
 * frame: the live window follows the page through this, not through the bridge's later message.
 */
internal class FrameWebView(
    context: Context,
) : WebView(context) {
    /** Called with the new scroll offset (device px) from inside the WebView's own scroll handling. */
    var onScroll: ScrollListener? = null

    /** The viewport `<meta>` the bridge gives the page ([PagePixels.viewportMeta]); read on the bridge's thread. */
    @Volatile
    var viewportMeta: String = ""

    /** Where the page is scrolled to and how far it can go (view px). */
    fun bounds(): ScrollBounds =
        ScrollBounds(
            scrollX,
            scrollY,
            (computeHorizontalScrollRange() - computeHorizontalScrollExtent()).coerceAtLeast(0),
            (computeVerticalScrollRange() - computeVerticalScrollExtent()).coerceAtLeast(0),
        )

    override fun onScrollChanged(
        l: Int,
        t: Int,
        oldl: Int,
        oldt: Int,
    ) {
        super.onScrollChanged(l, t, oldl, oldt)
        onScroll?.onScroll(l, t)
    }
}
