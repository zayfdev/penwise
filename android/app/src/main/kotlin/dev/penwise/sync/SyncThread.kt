package dev.penwise.sync

import android.util.Log
import java.util.concurrent.Executors
import java.util.concurrent.RejectedExecutionException
import java.util.concurrent.ScheduledExecutorService
import java.util.concurrent.ScheduledFuture
import java.util.concurrent.TimeUnit

private const val TAG = "PenwiseSync"
private val BACKOFF_SECONDS = longArrayOf(1, 2, 4, 8, 16, 30)

/**
 * The one thread every socket event and send runs on, so the connection needs no locks of its own. A task that
 * throws is logged and dropped: a bad frame or a native error must never kill the thread.
 */
internal class SyncThread {
    private val executor: ScheduledExecutorService =
        Executors.newSingleThreadScheduledExecutor { runnable ->
            Thread(runnable, "penwise-sync").apply { isDaemon = true }
        }

    /** Runs [task] soon; dropped after [shutdown]. */
    fun run(task: () -> Unit) {
        try {
            executor.execute { guard(task) }
        } catch (_: RejectedExecutionException) {
            Log.d(TAG, "sync stopped; a late callback was dropped")
        }
    }

    /** Runs [task] after [delay] [unit]; the future cancels it. Null (and dropped) after [shutdown]. */
    fun later(
        delay: Long,
        unit: TimeUnit,
        task: () -> Unit,
    ): ScheduledFuture<*>? =
        try {
            executor.schedule({ guard(task) }, delay, unit)
        } catch (_: RejectedExecutionException) {
            Log.d(TAG, "sync stopped; a late timer was dropped")
            null
        }

    fun shutdown() {
        executor.shutdown()
    }

    // Native errors and panics surface as RuntimeException; nothing finer can be caught here.
    @Suppress("TooGenericExceptionCaught")
    private fun guard(task: () -> Unit) {
        try {
            task()
        } catch (error: RuntimeException) {
            Log.w(TAG, "sync step failed: ${error.message}")
        }
    }
}

/**
 * Growing reconnect delay (1 s … 30 s) that also rotates through the configured endpoints.
 *
 * Rotation starts at the endpoint that last worked (the first one when none did), so a server restart is retried
 * where it was, and only repeated failures move on to the next endpoint.
 */
internal class Backoff {
    private var attempt = 0
    private var offset = 0
    private var good: String? = null
    private var justWelcomed = false

    /** A connection to [url] was established: start over with the shortest delay, at this endpoint. */
    fun welcomed(url: String) {
        attempt = 0
        offset = 0
        good = url
        justWelcomed = true
    }

    /** Connecting afresh: the shortest delay, at the last good endpoint, or at the first when [forgetGood]. */
    fun restart(forgetGood: Boolean = false) {
        attempt = 0
        offset = 0
        justWelcomed = false
        if (forgetGood) good = null
    }

    /** Index of the endpoint to dial next among [urls]. */
    fun endpoint(urls: List<String>): Int {
        val base = good?.let(urls::indexOf)?.takeIf { it >= 0 } ?: 0
        return (base + offset) % urls.size
    }

    /** Schedules [reconnect] after the current delay and moves to the next delay and endpoint. */
    fun schedule(
        thread: SyncThread,
        reconnect: () -> Unit,
    ): ScheduledFuture<*>? {
        val delay = BACKOFF_SECONDS[minOf(attempt, BACKOFF_SECONDS.lastIndex)]
        attempt++
        // A drop right after a good connection retries the same endpoint once; failures rotate.
        if (justWelcomed) justWelcomed = false else offset++
        return thread.later(delay, TimeUnit.SECONDS, reconnect)
    }
}

/** Fires [action] once the stream of events it watches has been quiet for [quietMs]; each [rearm] restarts the wait. */
internal class QuietTimer(
    private val thread: SyncThread,
    private val quietMs: Long,
    private val action: () -> Unit,
) {
    private var pending: ScheduledFuture<*>? = null

    fun rearm() {
        pending?.cancel(false)
        pending = thread.later(quietMs, TimeUnit.MILLISECONDS, action)
    }

    fun cancel() {
        pending?.cancel(false)
    }
}
