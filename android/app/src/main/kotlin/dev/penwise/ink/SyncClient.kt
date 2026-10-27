package dev.penwise.ink

import android.util.Log
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import java.util.concurrent.Executors
import java.util.concurrent.RejectedExecutionException
import java.util.concurrent.ScheduledExecutorService
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean

/**
 * Pushes the page to the Penwise PC server (JSON over WebSocket) so the browser tab mirrors it.
 *
 * Tries each configured URL in turn (home Wi-Fi first, Tailscale second), fire-and-forget: frames queue while
 * disconnected, the newest full page wins over stale increments, and it reconnects on its own. Never throws; a
 * broken link must not affect drawing.
 *
 * Two kinds of frame travel: durable ones (strokes, edits, page snapshots, selects) wait for a connection, and
 * ephemeral ones (live previews, erase sweeps) are only worth sending right now, so they are never queued while
 * disconnected and can never push a durable frame out of the queue.
 */
class SyncClient(
    private val urls: List<String>,
) {
    private val client =
        OkHttpClient
            .Builder()
            .pingInterval(PING_SECONDS, TimeUnit.SECONDS)
            .build()
    private val pending = ArrayDeque<Queued>()

    /**
     * The newest shape preview, kept out of [pending]: a drag produces one whole-shape frame per motion event,
     * and queueing them all makes the mirror replay a backlog (the shape keeps growing after the pen lifts).
     * Only the latest survives, and it is sent ahead of the queue so a commit never overtakes it.
     */
    private var pendingShape: String? = null

    /** Live samples wait here between ticks; one merged frame goes out instead of one per motion event. */
    private val liveBatch = LiveBatcher()
    private var liveOutSamples = FloatArray(0)
    private var liveOutTimes = IntArray(0)
    private val liveFlushQueued = AtomicBoolean(false)
    private val connecting = AtomicBoolean(false)

    /** Set by [stop]: late OkHttp callbacks of a stopped client must neither retry nor deliver frames. */
    private val stopped = AtomicBoolean(false)

    /** True between a successful open and the next close or failure; tests flip it to exercise the queue. */
    @Volatile
    internal var open = false
    private val retry: ScheduledExecutorService =
        Executors.newSingleThreadScheduledExecutor { runnable ->
            Thread(runnable, "penwise-sync").apply { isDaemon = true }
        }

    @Volatile
    private var socket: WebSocket? = null
    private var liveId = 0
    private var shapeId = 0

    /** Raw inbound text frames (page snapshots and broadcasts); the view model decides what to apply. */
    var onFrame: ((String) -> Unit)? = null

    /** Runs after every successful (re)connect, before queued frames flush. */
    var onConnect: (() -> Unit)? = null

    /** Page every outbound frame is scoped to; empty until the library has loaded. */
    @Volatile
    var pageId: String = ""
    private var next = 0

    fun start() {
        connect()
    }

    /**
     * Drops the connection and stops retrying (used when the address changes).
     *
     * The executor is not shut down abruptly: OkHttp reports the close we trigger here on its own threads, and
     * those callbacks reach [scheduleRetry]. They see [stopped] and return instead of hitting a dead executor.
     */
    fun stop() {
        stopped.set(true)
        open = false
        socket?.close(NORMAL_CLOSURE, "reconfigured")
        socket = null
        connecting.set(false)
        retry.shutdown()
        synchronized(pending) {
            pending.clear()
            pendingShape = null
        }
    }

    fun sendStroke(stroke: SavedStroke) {
        // Any samples still buffered belong to this stroke and must land before the committed stroke does.
        flushLive()
        enqueue(Wire.strokeFrame(stroke, pageId), durable = true)
    }

    /** Asks the server for `page`'s snapshot (sent after selecting a page and on connect). */
    fun sendSelect(page: String) {
        enqueue(Wire.select(page), durable = true)
    }

    /** Starts a live preview (frames are relayed, never stored). */
    fun sendLiveBegin(
        preset: Int,
        argb: Int,
        scale: Float,
        seed: Long,
    ) {
        flushLive()
        synchronized(liveBatch) { liveBatch.reset() }
        liveId++
        enqueue(Wire.liveBegin(liveId, preset, argb, scale, seed, pageId), durable = false)
    }

    /** Appends the samples the pen produced since the last call. */
    fun sendLiveSamples(
        samples: FloatArray,
        times: IntArray,
        count: Int,
    ) {
        if (count == 0) return
        synchronized(liveBatch) { liveBatch.add(liveId, samples, times, count) }
        if (liveFlushQueued.compareAndSet(false, true)) {
            if (!schedule(LIVE_FLUSH_MS, TimeUnit.MILLISECONDS) { flushLive() }) liveFlushQueued.set(false)
        }
    }

    /**
     * Sends whatever live samples are buffered, if any.
     *
     * One frame per tick merges every motion event in between: a pen reporting 120 Hz would otherwise queue
     * frames faster than the link drains them and the mirror would replay the backlog.
     */
    private fun flushLive() {
        liveFlushQueued.set(false)
        if (!liveBatch.pending) return
        synchronized(liveBatch) {
            val needed = liveBatch.pendingSamples
            if (liveOutTimes.size < needed) {
                liveOutSamples = FloatArray(needed * LIVE_STRIDE)
                liveOutTimes = IntArray(needed)
            }
            val id = liveBatch.strokeId
            val take = liveBatch.take(liveOutSamples, liveOutTimes)
            if (take == 0 || id < 0) return
            enqueue(Wire.liveSamples(id, liveOutSamples, liveOutTimes, take, pageId), durable = false)
        }
    }

    /** Withdraws the preview (committed strokes end it server-side). */
    fun sendLiveCancel() {
        if (liveId == 0) return
        // The stroke is gone: its buffered samples are worthless, so they are dropped rather than sent late.
        synchronized(liveBatch) { liveBatch.reset() }
        enqueue(Wire.liveCancel(liveId, pageId), durable = false)
        liveId = 0
    }

    /** Replaces the server's page; used on connect, undo, clear and erase. */
    fun sendPage(strokes: List<SavedStroke>) {
        flushLive()
        enqueue(Wire.page(strokes, pageId), durable = true)
    }

    /** Sends one incremental edit instead of the whole page. */
    fun sendOp(op: PageOp) {
        when (op) {
            is PageOp.Add -> op.strokes.forEach(::sendStroke)
            is PageOp.Remove -> sendRemove(op.seeds)
            is PageOp.Replace -> sendReplace(op.seeds, op.fragments)
            is PageOp.Clear -> sendClear()
        }
    }

    /** Small edits: stroke removal, page clear, fragment replacement. */
    fun sendRemove(seeds: List<Long>) {
        if (seeds.isNotEmpty()) enqueue(Wire.removeOp(seeds, pageId), durable = true)
    }

    /** Empties the page on the mirror. */
    fun sendClear() {
        enqueue(Wire.clearOp(pageId), durable = true)
    }

    /** Swaps erased strokes for their fragments. */
    fun sendReplace(
        seeds: List<Long>,
        strokes: List<SavedStroke>,
    ) {
        enqueue(Wire.replaceOp(seeds, strokes, pageId), durable = true)
    }

    /** Whole-set shape preview; the id only groups frames of one drag. */
    fun sendShapePreview(
        preset: Int,
        argb: Int,
        scale: Float,
        seed: Long,
        samples: FloatArray,
        times: IntArray,
        count: Int,
    ) {
        if (!open) return
        shapeId++
        flushLive()
        synchronized(pending) {
            pendingShape = Wire.shapePreview(shapeId, preset, argb, scale, seed, samples, times, count, pageId)
        }
        flush()
    }

    /** Where the eraser is, so the mirror can show the sweep. */
    fun sendErasePreview(
        x: Float,
        y: Float,
        radius: Float,
    ) {
        enqueue(Wire.erasePreview(x, y, radius, pageId), durable = false)
    }

    private fun enqueue(
        frame: String,
        durable: Boolean,
    ) {
        // A preview of something happening now is worthless later: never queue one while the link is down.
        if (!durable && !open) return
        synchronized(pending) {
            pending.addLast(Queued(frame, durable))
            trim()
        }
        flush()
    }

    /** Keeps the queue bounded, shedding previews before anything durable. */
    private fun trim() {
        while (pending.count { !it.durable } > MAX_EPHEMERAL) pending.removeAt(pending.indexOfFirst { !it.durable })
        while (pending.size > MAX_DURABLE) {
            val victim = pending.indexOfFirst { !it.durable }.takeIf { it >= 0 } ?: 0
            pending.removeAt(victim)
        }
    }

    /** Frames still waiting to go out, in order; exposed for tests. */
    internal fun queuedFrames(): List<String> = synchronized(pending) { pending.map { it.text } }

    private fun flush() {
        val link = socket
        if (link == null || !open) return
        // Freshest preview first, then commits and strokes in order.
        synchronized(pending) { pendingShape }?.let { frame ->
            if (link.send(frame)) synchronized(pending) { if (pendingShape == frame) pendingShape = null }
        }
        while (true) {
            val frame = synchronized(pending) { pending.removeFirstOrNull() } ?: return
            if (!link.send(frame.text)) {
                synchronized(pending) { pending.addFirst(frame) }
                return
            }
        }
    }

    private fun connect() {
        if (stopped.get() || urls.isEmpty() || socket != null || !connecting.compareAndSet(false, true)) return
        val url = urls[next % urls.size]
        Log.i(TAG, "connecting to $url")
        socket = client.newWebSocket(Request.Builder().url(url).build(), listener)
    }

    /** Rotates to the next configured address, then retries; a stopped client does neither. */
    private fun scheduleRetry() {
        open = false
        if (stopped.get()) return
        socket = null
        connecting.set(false)
        next++
        schedule(RETRY_SECONDS, TimeUnit.SECONDS) { connect() }
    }

    /** Runs `task` later on the sync thread; false when the executor is already shut down (never throws). */
    private fun schedule(
        delay: Long,
        unit: TimeUnit,
        task: () -> Unit,
    ): Boolean =
        try {
            retry.schedule(task, delay, unit)
            true
        } catch (_: RejectedExecutionException) {
            false
        }

    /** Callbacks OkHttp makes on its own threads; internal so tests can drive them. */
    internal val listener =
        object : WebSocketListener() {
            // Boundary guard: a frame that cannot be applied must not kill OkHttp's reader thread.
            @Suppress("TooGenericExceptionCaught")
            override fun onMessage(
                webSocket: WebSocket,
                text: String,
            ) {
                if (stopped.get()) return
                try {
                    onFrame?.invoke(text)
                } catch (error: RuntimeException) {
                    Log.w(TAG, "dropped frame: ${error.message}")
                }
            }

            override fun onOpen(
                webSocket: WebSocket,
                response: Response,
            ) {
                connecting.set(false)
                if (stopped.get()) return
                open = true
                Log.i(TAG, "connected")
                onConnect?.invoke()
                flush()
            }

            override fun onClosed(
                webSocket: WebSocket,
                code: Int,
                reason: String,
            ) = scheduleRetry()

            override fun onFailure(
                webSocket: WebSocket,
                throwable: Throwable,
                response: Response?,
            ) {
                if (!stopped.get()) Log.w(TAG, "connection failed: ${throwable.message}")
                scheduleRetry()
            }
        }

    private class Queued(
        val text: String,
        val durable: Boolean,
    )

    private companion object {
        const val TAG = "PenwiseSync"
        const val PING_SECONDS = 20L
        const val RETRY_SECONDS = 3L
        const val MAX_DURABLE = 1024
        const val MAX_EPHEMERAL = 64

        /** Delay before a buffered live batch is sent; one or two display frames. */
        const val LIVE_FLUSH_MS = 16L
        const val LIVE_STRIDE = 5
        const val NORMAL_CLOSURE = 1000
    }
}
