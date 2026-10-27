package dev.penwise.sync

import android.util.Log
import java.io.IOException
import java.util.concurrent.Executors
import java.util.concurrent.Future
import java.util.concurrent.atomic.AtomicBoolean

private const val TAG = "PenwiseSyncState"

/**
 * Writes the core's state to [file] after every change, off the pen thread.
 *
 * Requests coalesce: while one write runs, any number of further requests produce exactly one more write that
 * exports the newest state. [flush] blocks until everything requested so far is on disk (lifecycle stop, tests).
 */
class StatePersister(
    private val core: SyncCore,
    private val file: SyncStateFile,
) : AutoCloseable {
    private val writer =
        Executors.newSingleThreadExecutor { runnable ->
            Thread(runnable, "penwise-state").apply { isDaemon = true }
        }
    private val queued = AtomicBoolean(false)
    private val lastRequest =
        java.util.concurrent.atomic
            .AtomicReference<Future<*>?>(null)

    /** Schedules a write of the current state. */
    fun request() {
        if (!queued.compareAndSet(false, true)) return
        lastRequest.set(writer.submit(::persist))
    }

    private fun persist() {
        queued.set(false)
        try {
            file.write(core.exportState())
        } catch (error: IOException) {
            // The outbox stays in memory and the next change retries; a full disk must not crash the pen path.
            Log.w(TAG, "state write failed: ${error.message}")
        }
    }

    /** Waits for every requested write to finish. */
    fun flush() {
        lastRequest.get()?.get()
    }

    override fun close() {
        flush()
        writer.shutdown()
    }
}
