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

    /** Runs [task] after [delay] [unit]; the future cancels it. */
    fun later(
        delay: Long,
        unit: TimeUnit,
        task: () -> Unit,
    ): ScheduledFuture<*> = executor.schedule({ guard(task) }, delay, unit)

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

/** Growing reconnect delay (1 s … 30 s) that also rotates through the configured endpoints. */
internal class Backoff {
    private var attempt = 0
    private var rotation = 0

    /** A connection was established: start over with the shortest delay. */
    fun reset() {
        attempt = 0
    }

    /** Index of the endpoint to dial next among `count`. */
    fun endpoint(count: Int): Int = rotation % count

    /** Schedules [reconnect] after the current delay and moves to the next delay and endpoint. */
    fun schedule(
        thread: SyncThread,
        reconnect: () -> Unit,
    ): ScheduledFuture<*> {
        val delay = BACKOFF_SECONDS[minOf(attempt, BACKOFF_SECONDS.lastIndex)]
        attempt++
        rotation++
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
