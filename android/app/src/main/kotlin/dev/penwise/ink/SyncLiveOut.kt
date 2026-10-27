package dev.penwise.ink

import dev.penwise.sync.SyncCore
import java.util.concurrent.Executors
import java.util.concurrent.ScheduledExecutorService
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean

private const val FLUSH_MS = 16L
private const val CHUNK_SAMPLES = 256
private const val STRIDE = 5

/**
 * Streams the pen's in-progress stroke to the server as `StrokeDelta` frames (droppable: nothing is queued while
 * offline) so the browser shows it before the pen lifts. The frame encoding is Rust ([SyncCore.liveDelta]); this
 * class only batches samples per tick and keeps the sample index.
 *
 * The input thread only appends to the batch; frames are built and sent on one scheduler thread, so a cancel can
 * never overtake a delta.
 */
internal class SyncLiveOut(
    private val core: SyncCore,
    private val send: (ByteArray) -> Unit,
    private val scheduler: ScheduledExecutorService =
        Executors.newSingleThreadScheduledExecutor { runnable ->
            Thread(runnable, "penwise-live").apply { isDaemon = true }
        },
) : LiveOut {
    private val batch = LiveBatcher()
    private val flushQueued = AtomicBoolean(false)
    private var stroke: Preview? = null
    private var outSamples = FloatArray(CHUNK_SAMPLES * STRIDE)
    private var outTimes = IntArray(CHUNK_SAMPLES)

    /** Brush and identity of the stroke being previewed. */
    private class Preview(
        val page: ByteArray,
        val id: ByteArray,
        val brush: IntArray,
        val scale: Float,
        val seed: Long,
    )

    override fun begin(
        page: ByteArray,
        id: ByteArray,
        preset: Int,
        argb: Int,
        scale: Float,
        seed: Long,
    ) {
        synchronized(batch) {
            batch.reset()
            stroke = Preview(page, id, intArrayOf(preset, argb), scale, seed)
        }
    }

    override fun samples(
        samples: FloatArray,
        times: IntArray,
        count: Int,
    ) {
        synchronized(batch) {
            if (stroke == null) return
            batch.add(samples, times, count)
        }
        if (flushQueued.compareAndSet(false, true)) scheduler.schedule(::flush, FLUSH_MS, TimeUnit.MILLISECONDS)
    }

    override fun end(cancelled: Boolean) {
        val finished =
            synchronized(batch) {
                batch.reset()
                stroke.also { stroke = null }
            }
        // A committed stroke ends the preview server-side; only a withdrawn one needs a frame.
        if (cancelled && finished != null) scheduler.execute { send(core.liveCancel(finished.page, finished.id)) }
    }

    private fun flush() {
        flushQueued.set(false)
        while (true) {
            val (live, first, taken) =
                synchronized(batch) {
                    val live = stroke ?: return
                    val first = batch.start
                    Triple(live, first, batch.take(outSamples, outTimes, CHUNK_SAMPLES))
                }
            if (taken == 0) return
            send(
                core.liveDelta(
                    live.page,
                    live.id,
                    live.brush,
                    live.scale,
                    live.seed,
                    first,
                    outSamples.copyOf(taken * STRIDE),
                    outTimes.copyOf(taken),
                ),
            )
        }
    }
}
