package dev.penwise.frames

/**
 * A frame to open on the canvas: the live page of [project] at [route], shown at [preset]'s CSS size and placed at a
 * page position. The layer multiplies the CSS size by the display density, so the page's breakpoints see the preset
 * and the canvas sees device px.
 *
 * [url] is the one the WebView loads: the project's own port on the host the tablet syncs with (see [FrameUrls]).
 */
data class FrameSpec(
    val id: String,
    val project: String,
    val route: String,
    val preset: String,
    val url: String,
    val x: Float,
    val y: Float,
    val cssWidth: Int,
    val cssHeight: Int,
)

/** What the page inside a frame reports through the bridge. */
data class FrameScroll(
    val x: Int,
    val y: Int,
    val docWidth: Int,
    val docHeight: Int,
) {
    companion object {
        val NONE = FrameScroll(0, 0, 0, 0)
    }
}
