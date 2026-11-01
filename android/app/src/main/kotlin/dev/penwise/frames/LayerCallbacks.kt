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

    private var focused: String? = null
    private var locked: String? = null

    /**
     * The live frame changed (null: none is live). A listener that is set after a change still hears the current
     * live frame at once, so the order the stage wires its callbacks in cannot lose the first one.
     */
    var onFocused: ((String?) -> Unit)? = null
        set(value) {
            field = value
            if (focused != null) value?.invoke(focused)
        }

    /** The locked frame changed (null: no lock). Replays the current lock to a listener set later, like [onFocused]. */
    var onLocked: ((String?) -> Unit)? = null
        set(value) {
            field = value
            if (locked != null) value?.invoke(locked)
        }

    /** The live frame is now [id]. */
    fun focusChanged(id: String?) {
        focused = id
        onFocused?.invoke(id)
    }

    /** The locked frame is now [id]. */
    fun lockChanged(id: String?) {
        locked = id
        onLocked?.invoke(id)
    }

    /** A pinch past the fit scale, or the agent, asks for the overview. */
    var onOpenOverview: (() -> Unit)? = null

    /** The owner double-tapped a frame: the camera fits it, and other devices may be told. */
    var onUserFocus: ((String) -> Unit)? = null
}
