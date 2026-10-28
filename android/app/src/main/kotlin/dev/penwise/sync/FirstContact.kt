package dev.penwise.sync

/**
 * Decides when the first run of the app has found no server at all: only once *every* endpoint failed (or there
 * is none). The LAN address times out away from home while Tailscale may well answer, so one failure says nothing.
 * Touched only on the sync thread.
 */
internal class FirstContact(
    private val endpoints: () -> List<String>,
    private val onUnreachable: () -> Unit,
) {
    private val failed = HashSet<String>()
    private var settled = false

    /** A server answered; "unreachable" is never reported after that. */
    fun welcomed() {
        settled = true
    }

    /** [url] failed (null: there was nothing to dial). */
    fun failed(url: String?) {
        if (settled) return
        url?.let(failed::add)
        if (failed.containsAll(endpoints())) {
            settled = true
            onUnreachable()
        }
    }
}
