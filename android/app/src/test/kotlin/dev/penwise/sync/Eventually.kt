package dev.penwise.sync

private const val POLL_MS = 25L
private const val DEFAULT_TIMEOUT_MS = 15_000L

/** Polls until [condition] holds; fails with [what] after the timeout. */
fun eventually(
    what: String,
    timeoutMs: Long = DEFAULT_TIMEOUT_MS,
    condition: () -> Boolean,
) {
    val deadline = System.currentTimeMillis() + timeoutMs
    while (!condition()) {
        check(System.currentTimeMillis() < deadline) { "timed out waiting for: $what" }
        Thread.sleep(POLL_MS)
    }
}

/** A [SyncListener] that remembers what it saw. */
class Recorder : SyncListener {
    val outcomes = java.util.concurrent.CopyOnWriteArrayList<Outcome>()
    val statuses = java.util.concurrent.CopyOnWriteArrayList<SyncStatus>()

    @Volatile
    var caughtUp = 0

    @Volatile
    var unreachable = 0

    override fun onOutcome(outcome: Outcome) {
        outcomes.add(outcome)
    }

    override fun onStatus(status: SyncStatus) {
        statuses.add(status)
    }

    override fun onCaughtUp() {
        caughtUp++
    }

    override fun onUnreachable() {
        unreachable++
    }
}
