package dev.penwise.sync

import android.util.Log
import java.io.IOException
import java.util.concurrent.Callable
import java.util.concurrent.ExecutionException
import java.util.concurrent.RejectedExecutionException
import java.util.concurrent.ScheduledFuture
import java.util.concurrent.ScheduledThreadPoolExecutor
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicLong

private const val TAG = "PenwiseSyncState"
private val RETRY_MS = longArrayOf(1_000, 2_000, 4_000, 8_000, 16_000, 30_000)

/**
 * Writes the core's state to disk after every change, off the pen thread and off the core lock (the lock is held
 * only to clone the state).
 *
 * Requests coalesce: while one write runs, any number of further requests produce one more write of the newest
 * state. A failed write is not forgotten: [failing] turns true, [onFailing] is told, and the write is retried with
 * a growing delay until it succeeds, whether or not another change comes. [flush] runs its own write and says
 * whether the state is on disk.
 *
 * @param write where the encoded state goes (the state file; replaceable for tests)
 */
class StatePersister(
    private val core: SyncCore,
    file: SyncStateFile,
    private val onFailing: (Boolean) -> Unit = {},
    private val retryMs: LongArray = RETRY_MS,
    private val write: (ByteArray) -> Unit = file::write,
) : AutoCloseable {
    private val worker =
        ScheduledThreadPoolExecutor(1) { runnable ->
            Thread(runnable, "penwise-state").apply { isDaemon = true }
        }.apply { executeExistingDelayedTasksAfterShutdownPolicy = false }
    private val requested = AtomicLong(0)
    private val queued = AtomicBoolean(false)

    // Worker thread only.
    private var written = 0L
    private var writtenRevision = -1L
    private var failures = 0
    private var retry: ScheduledFuture<*>? = null

    /** True while the last write failed; the UI shows an error for it. */
    @Volatile
    var failing = false
        private set

    /** Schedules a write of the current state. */
    fun request() {
        requested.incrementAndGet()
        if (!queued.compareAndSet(false, true)) return
        try {
            worker.execute {
                queued.set(false)
                attempt()
            }
        } catch (_: RejectedExecutionException) {
            queued.set(false)
        }
    }

    /** One write of everything requested so far, unless it is already on disk. True when the disk is current. */
    @Suppress("TooGenericExceptionCaught") // a native export failure (RuntimeException) must not kill the writer
    private fun attempt(): Boolean {
        val target = requested.get()
        if (target == written && !failing) return true
        return try {
            val revision = core.revision()
            if (revision != writtenRevision || failing) {
                write(core.exportStateOffLock())
                writtenRevision = revision
            }
            written = target
            failures = 0
            retry?.cancel(false)
            retry = null
            setFailing(false)
            true
        } catch (error: IOException) {
            failed(error)
        } catch (error: RuntimeException) {
            failed(error)
        }
    }

    private fun failed(error: Exception): Boolean {
        Log.w(TAG, "state write failed: ${error.message}")
        setFailing(true)
        if (retry == null) {
            val delay = retryMs[minOf(failures, retryMs.lastIndex)]
            failures++
            retry =
                try {
                    worker.schedule(
                        {
                            retry = null
                            attempt()
                        },
                        delay,
                        TimeUnit.MILLISECONDS,
                    )
                } catch (_: RejectedExecutionException) {
                    null
                }
        }
        return false
    }

    private fun setFailing(value: Boolean) {
        if (failing == value) return
        failing = value
        onFailing(value)
    }

    /** Writes the current state now and waits for that very write; true when it is on disk. */
    fun flush(): Boolean =
        try {
            worker.submit(Callable { attempt() }).get()
        } catch (_: ExecutionException) {
            false
        } catch (_: RejectedExecutionException) {
            false
        } catch (_: InterruptedException) {
            Thread.currentThread().interrupt()
            false
        }

    override fun close() {
        flush()
        worker.shutdown()
    }
}
