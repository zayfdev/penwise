package dev.penwise.frames

/**
 * The two places a frame's page can be: live (the one the owner works in) and warm (the one that was live before,
 * hidden and paused for a quick return). Anything older is destroyed. What a page is stays with the caller (`open`,
 * `wake`, `park`, `destroy`), so the bookkeeping is tested without a WebView. UI thread only.
 */
internal class PageSlots<T : Any>(
    private val idOf: (T) -> String,
    private val open: (FrameSpec) -> T,
    private val wake: (T) -> Unit,
    private val park: (T) -> Unit,
    private val destroy: (T) -> Unit,
) {
    var live: T? = null
        private set

    var warm: T? = null
        private set

    fun entry(id: String): T? = live?.takeIf { idOf(it) == id } ?: warm?.takeIf { idOf(it) == id }

    fun all(): List<T> = listOfNotNull(live, warm)

    /**
     * Makes [spec] the live page (null: none). The page that was live is parked warm and the warm one before it is
     * destroyed; a page that is already warm is woken instead of opened. Returns the page that stopped being live.
     */
    fun focus(spec: FrameSpec?): T? {
        val before = live
        if (before?.let(idOf) == spec?.id) return null
        val next = spec?.let { wanted -> takeWarm(wanted.id) ?: open(wanted) }
        if (before != null) {
            warm?.let(destroy)
            park(before)
            warm = before
        }
        live = next
        return before
    }

    /** A frame is gone: its page is destroyed whether it was live or warm. */
    fun close(id: String) {
        if (live?.let(idOf) == id) {
            live?.let(destroy)
            live = null
        }
        if (warm?.let(idOf) == id) {
            warm?.let(destroy)
            warm = null
        }
    }

    /** Destroys every page (page leave). */
    fun destroyAll() {
        all().forEach(destroy)
        live = null
        warm = null
    }

    private fun takeWarm(id: String): T? {
        val page = warm?.takeIf { idOf(it) == id } ?: return null
        warm = null
        wake(page)
        return page
    }
}
