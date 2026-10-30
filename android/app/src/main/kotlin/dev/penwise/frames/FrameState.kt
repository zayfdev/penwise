package dev.penwise.frames

/**
 * A frame to open on the canvas: the live page of [project] at [route], shown at [preset]'s CSS size and placed at a
 * page position. The layer multiplies the CSS size by the display density, so the page's breakpoints see the preset
 * and the canvas sees device px.
 *
 * [boxHeight] is the box on the board (the whole document for a [full] frame); the WebView of the live frame stays
 * [cssWidth] × [cssHeight], the preset.
 *
 * [url] is the one the WebView loads: the project's own port on the host the tablet syncs with (see [FrameUrls]).
 *
 * [patch] is the ghost/variant patch (P1 JSON text) the page applies; [label] and [group] name a variant, and
 * [chosen] is the owner's pick within its group (the core un-chooses the others).
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
    val full: Boolean = false,
    val docHeight: Int = 0,
    val boxHeight: Int = cssHeight,
    val patch: String? = null,
    val label: String = "",
    val group: String? = null,
    val chosen: Boolean = false,
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
