package dev.penwise.ink

import android.util.Log
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import java.util.concurrent.Executors
import java.util.concurrent.ScheduledExecutorService
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean

/**
 * Pushes the page to the Penwise PC server (JSON over WebSocket) so the browser tab mirrors it.
 *
 * Tries each configured URL in turn (home Wi-Fi first, Tailscale second), fire-and-forget: frames queue while
 * disconnected, the newest full page wins over stale increments, and it reconnects on its own. Never throws; a
 * broken link must not affect drawing.
 */
class SyncClient(
    private val urls: List<String>,
) {
    private val client =
        OkHttpClient
            .Builder()
            .pingInterval(PING_SECONDS, TimeUnit.SECONDS)
            .build()
    private val pending = ArrayDeque<String>()

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

    /** Drops the connection and stops retrying (used when the address changes). */
    fun stop() {
        retry.shutdownNow()
        socket?.close(NORMAL_CLOSURE, "reconfigured")
        socket = null
        connecting.set(false)
        synchronized(pending) {
            pending.clear()
            pendingShape = null
        }
    }

    fun sendStroke(stroke: SavedStroke) {
        // Any samples still buffered belong to this stroke and must land before the committed stroke does.
        flushLive()
        enqueue(Wire.strokeFrame(stroke, pageId))
    }

    /** Asks the server for `page`'s snapshot (sent after selecting a page and on connect). */
    fun sendSelect(page: String) {
        enqueue(Wire.select(page))
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
        enqueue(Wire.liveBegin(liveId, preset, argb, scale, seed, pageId))
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
            retry.schedule({ flushLive() }, LIVE_FLUSH_MS, TimeUnit.MILLISECONDS)
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
            enqueue(Wire.liveSamples(id, liveOutSamples, liveOutTimes, take, pageId))
        }
    }

    /** Withdraws the preview (committed strokes end it server-side). */
    fun sendLiveCancel() {
        if (liveId == 0) return
        // The stroke is gone: its buffered samples are worthless, so they are dropped rather than sent late.
        synchronized(liveBatch) { liveBatch.reset() }
        enqueue(Wire.liveCancel(liveId, pageId))
        liveId = 0
    }

    /** Replaces the server's page; used on connect, undo, clear and erase. */
    fun sendPage(strokes: List<SavedStroke>) {
        flushLive()
        enqueue(Wire.page(strokes, pageId))
    }

    /** Small edits: stroke removal, page clear, fragment replacement. */
    fun sendRemove(seeds: List<Long>) {
        if (seeds.isNotEmpty()) enqueue(Wire.removeOp(seeds, pageId))
    }

    /** Empties the page on the mirror. */
    fun sendClear() {
        enqueue(Wire.clearOp(pageId))
    }

    /** Swaps erased strokes for their fragments. */
    fun sendReplace(
        seeds: List<Long>,
        strokes: List<SavedStroke>,
    ) {
        enqueue(Wire.replaceOp(seeds, strokes, pageId))
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
        enqueue(Wire.erasePreview(x, y, radius, pageId))
    }

    private fun enqueue(frame: String) {
        synchronized(pending) {
            if (pending.size >= MAX_PENDING) pending.removeFirst()
            pending.addLast(frame)
        }
        flush()
    }

    private fun flush() {
        val open = socket ?: return
        // Freshest preview first, then commits and strokes in order.
        synchronized(pending) { pendingShape }?.let { frame ->
            if (open.send(frame)) synchronized(pending) { if (pendingShape == frame) pendingShape = null }
        }
        while (true) {
            val frame = synchronized(pending) { pending.removeFirstOrNull() } ?: return
            if (!open.send(frame)) return
        }
    }

    private fun connect() {
        if (urls.isEmpty() || !connecting.compareAndSet(false, true)) return
        val url = urls[next % urls.size]
        Log.i(TAG, "connecting to $url")
        socket = client.newWebSocket(Request.Builder().url(url).build(), listener)
    }

    /** Rotates to the next configured address, then retries. */
    private fun scheduleRetry() {
        socket = null
        connecting.set(false)
        next++
        retry.schedule({ connect() }, RETRY_SECONDS, TimeUnit.SECONDS)
    }

    private val listener =
        object : WebSocketListener() {
            override fun onMessage(
                webSocket: WebSocket,
                text: String,
            ) {
                onFrame?.invoke(text)
            }

            override fun onOpen(
                webSocket: WebSocket,
                response: Response,
            ) {
                connecting.set(false)
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
                Log.w(TAG, "connection failed: ${throwable.message}")
                scheduleRetry()
            }
        }

    private companion object {
        const val TAG = "PenwiseSync"
        const val PING_SECONDS = 20L
        const val RETRY_SECONDS = 3L
        const val MAX_PENDING = 64

        /** Delay before a buffered live batch is sent; one or two display frames. */
        const val LIVE_FLUSH_MS = 16L
        const val LIVE_STRIDE = 5
        const val NORMAL_CLOSURE = 1000
    }
}
