package dev.penwise.frames

/** What the frame layer tells the screen around it. All on the UI thread; set by the stage. */
internal class LayerCallbacks {
    /** A page reported a new scroll offset (the ink layer reads it for area marks). */
    var onScroll: ((String, FrameScroll) -> Unit)? = null

    /**
     * The user's own navigation inside a page (a link click) landed on another route, at most once per settled
     * navigation: the frame item's `route` follows, without a reload.
     */
    var onRoute: ((String, String) -> Unit)? = null

    /**
     * A layout the core should take: the frame id, the layout as the core takes it (its preset is filled here),
     * where it comes from, and the page's state recipe ("" = the initial UI).
     */
    var onLayout: ((frameId: String, json: String, kind: LayoutKind, state: String) -> Unit)? = null

    /** The live frame changed (null: none is live). */
    var onFocused: ((String?) -> Unit)? = null

    /** The owner double-tapped a frame: the camera fits it, and other devices may be told. */
    var onUserFocus: ((String) -> Unit)? = null
}
