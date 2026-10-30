package dev.penwise.frames

import kotlinx.coroutines.delay

/**
 * Send waits for the local outbox to be acknowledged before it posts, so the stroke drawn a moment ago belongs to the
 * round it was drawn for (contract §7). It waits at most [maxMs]; after that the round is sent anyway (a server that
 * does not acknowledge would otherwise keep the owner from sending at all).
 */
internal object OutboxWait {
    /** How long Send waits for the outbox at most. */
    const val MAX_MS = 3_000L
    private const val POLL_MS = 50L

    /**
     * Polls [pending] (ops still waiting for an `Ack`) until it reaches zero or [maxMs] have passed on [clock]; true
     * when the outbox was acknowledged, false when the wait ran out. [pause] sleeps between polls.
     */
    suspend fun untilAcked(
        pending: () -> Int,
        maxMs: Long = MAX_MS,
        clock: () -> Long = { System.nanoTime() / NANOS_PER_MS },
        pause: suspend (Long) -> Unit = { delay(it) },
    ): Boolean {
        val deadline = clock() + maxMs
        while (pending() > 0) {
            if (clock() >= deadline) return false
            pause(POLL_MS)
        }
        return true
    }

    private const val NANOS_PER_MS = 1_000_000L
}
