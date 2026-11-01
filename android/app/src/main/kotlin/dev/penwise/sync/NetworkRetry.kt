package dev.penwise.sync

/** Tells when a usable network shows up; the Android side is [AndroidNetworkSource], tests use a fake. */
fun interface NetworkSource {
    /** Calls [onValidated] with the network's identity each time a network becomes usable; close to stop. */
    fun watch(onValidated: (network: Any) -> Unit): AutoCloseable

    /** The network traffic would use right now, or null when unknown. */
    fun active(): Any? = null
}

/**
 * When, and whether, to redial after a network turns up. The socket's own backoff can be 30 s deep by then, so a
 * usable network means "try now"; but networks flap, so events are debounced (one redial after the last of a burst,
 * at least [minGapMs] after the previous redial) and judged only when it is time: a socket that is connecting or
 * healthy on the same network is never torn down.
 */
internal class NetworkRetry(
    private val clock: () -> Long,
    private val debounceMs: Long = DEBOUNCE_MS,
    private val minGapMs: Long = MIN_GAP_MS,
) {
    /** The network that most recently became usable. */
    var latest: Any? = null
        private set

    private var lastRedial: Long? = null

    /** A network became usable: the wait in ms before looking at the socket (the caller replaces its pending look). */
    fun onValidated(network: Any): Long {
        latest = network
        val earliest = lastRedial?.let { it + minGapMs - clock() } ?: 0L
        return maxOf(debounceMs, earliest)
    }

    /** The redial happened now. */
    fun redialed() {
        lastRedial = clock()
    }

    /**
     * Whether to redial now. A socket that is waiting out its backoff always; one that is mid-dial never; an online
     * one only when it was opened on another network than [latest] (`openedOn` null: unknown, so left alone).
     */
    fun shouldRedial(
        hasSocket: Boolean,
        online: Boolean,
        openedOn: Any?,
    ): Boolean =
        when {
            !hasSocket -> true
            !online -> false
            else -> openedOn != null && openedOn != latest
        }

    internal companion object {
        const val DEBOUNCE_MS = 300L
        const val MIN_GAP_MS = 2_000L
    }
}
