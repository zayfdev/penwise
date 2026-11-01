package dev.penwise.overview

import dev.penwise.frames.LaneKey
import dev.penwise.frames.OutboxWait
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

/**
 * Attaches new sketch pages to their strips on the server (contract §2.3). A page is created locally and reaches the
 * server through sync, so the PUT first waits for the outbox to be acknowledged, then retries a `404` (the page is not
 * there yet) with a growing pause. What cannot be attached yet (offline, still unknown) stays in [pending], in memory
 * for as long as the app runs, and every [flush] (the overview opening) sends it again.
 */
internal class SketchAttacher(
    private val scope: CoroutineScope,
    private val send: (lane: LaneKey, page: String) -> AttachResult,
    private val outbox: () -> Int,
    private val onAttached: () -> Unit = {},
    private val io: CoroutineDispatcher = Dispatchers.IO,
    private val pause: suspend (Long) -> Unit = { delay(it) },
    private val untilAcked: suspend (() -> Int) -> Unit = { OutboxWait.untilAcked(it) },
) {
    private val lock = Any()
    private val queue = LinkedHashMap<String, LaneKey>()
    private var running = false
    private var again = false

    /** The pages still to attach, page to strip. */
    val pending: Map<String, LaneKey> get() = synchronized(lock) { LinkedHashMap(queue) }

    /** A page was created for [lane]: it is remembered and sent. */
    fun add(
        lane: LaneKey,
        page: String,
    ) {
        synchronized(lock) { queue[page] = lane }
        flush()
    }

    /** Sends everything that is pending, off the main thread; a run already going on picks up what was added. */
    fun flush() {
        synchronized(lock) {
            if (running) {
                again = true
                return
            }
            running = true
        }
        scope.launch(io) {
            try {
                do {
                    synchronized(lock) { again = false }
                    flushNow()
                } while (synchronized(lock) { again.also { running = it } })
            } finally {
                synchronized(lock) { running = false }
            }
        }
    }

    /** One pass over the pending pages; stops at the first one the server could not be reached for. */
    suspend fun flushNow() {
        for ((page, lane) in pending) {
            untilAcked(outbox)
            when (attempt(lane, page)) {
                AttachResult.OK -> done(page, attached = true)
                AttachResult.REJECTED -> done(page, attached = false)
                AttachResult.NOT_FOUND -> Unit
                AttachResult.FAILED -> return
            }
        }
    }

    private suspend fun attempt(
        lane: LaneKey,
        page: String,
    ): AttachResult {
        var result = send(lane, page)
        for (wait in BACKOFF_MS) {
            if (result != AttachResult.NOT_FOUND) break
            pause(wait)
            result = send(lane, page)
        }
        return result
    }

    private fun done(
        page: String,
        attached: Boolean,
    ) {
        synchronized(lock) { queue.remove(page) }
        if (attached) onAttached()
    }

    private companion object {
        /** Pauses before the retries of a `404`. */
        val BACKOFF_MS = longArrayOf(500, 1_000, 2_000)
    }
}
