package dev.penwise.ink

import java.util.concurrent.Executor
import java.util.concurrent.atomic.AtomicLong
import java.util.concurrent.atomic.AtomicReference

/**
 * Runs one background render at a time and keeps only the newest result: [submit] supersedes whatever was requested
 * before, [cancel] drops it, and a result that is no longer current is passed to [dispose] instead of being
 * published. [render] runs on the [executor] and should poll its `current` argument to stop early. Never blocks the
 * caller: [submit], [cancel] and [take] only touch atomics. [onReady] runs on the executor thread when a current
 * result is waiting for [take]. A [render] that throws is reported to [onError] and counts as no result.
 */
internal class LatestRenderer<Req, Res : Any>(
    private val executor: Executor,
    private val render: (Req, current: () -> Boolean) -> Res?,
    private val dispose: (Res) -> Unit,
    private val onReady: () -> Unit,
    private val onError: (Throwable) -> Unit = {},
) {
    private class Done<Res>(
        val generation: Long,
        val result: Res,
    )

    private val generation = AtomicLong()
    private val ready = AtomicReference<Done<Res>?>()

    @Volatile
    private var running = NONE

    /** True while a request is in flight (rendering, or finished and not yet taken). */
    val pending: Boolean get() = running != NONE

    /** Requests a render of [request]; any earlier request or unclaimed result becomes stale. */
    fun submit(request: Req) {
        val mine = generation.incrementAndGet()
        running = mine
        ready.getAndSet(null)?.let { dispose(it.result) }
        executor.execute { run(mine, request) }
    }

    /** Makes everything requested so far stale. */
    fun cancel() {
        generation.incrementAndGet()
        running = NONE
        ready.getAndSet(null)?.let { dispose(it.result) }
    }

    /** The finished current result, once; null when there is none (stale results never come out). */
    fun take(): Res? {
        val done = ready.getAndSet(null) ?: return null
        if (done.generation == generation.get()) {
            running = NONE
            return done.result
        }
        dispose(done.result)
        return null
    }

    private fun run(
        mine: Long,
        request: Req,
    ) {
        val current = { generation.get() == mine }
        if (!current()) return
        val result =
            try {
                render(request, current)
            } catch (
                @Suppress("TooGenericExceptionCaught") error: Throwable,
            ) {
                // Out of memory or a malformed item: never let it reach the thread's uncaught handler, and never
                // leave `running` set, or the caller waits for a result that will not come.
                onError(error)
                null
            } ?: return finishRun(mine)
        if (!current()) {
            dispose(result)
            return
        }
        val previous = ready.getAndSet(Done(mine, result))
        previous?.let { dispose(it.result) }
        // A cancel that slipped in between the check and the publish must not leave its result behind.
        if (!current()) {
            ready.getAndSet(null)?.let { dispose(it.result) }
            return
        }
        onReady()
    }

    private fun finishRun(mine: Long) {
        if (generation.get() == mine) running = NONE
    }

    private companion object {
        const val NONE = 0L
    }
}
