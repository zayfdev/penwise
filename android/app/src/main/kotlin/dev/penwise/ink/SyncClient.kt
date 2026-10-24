package dev.penwise.ink

import android.util.Log
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import org.json.JSONArray
import org.json.JSONObject
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
    private val connecting = AtomicBoolean(false)
    private val retry: ScheduledExecutorService =
        Executors.newSingleThreadScheduledExecutor { runnable ->
            Thread(runnable, "penwise-sync").apply { isDaemon = true }
        }

    @Volatile
    private var socket: WebSocket? = null
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
        synchronized(pending) { pending.clear() }
    }

    fun sendStroke(stroke: SavedStroke) {
        enqueue(JSONObject().put(T, STROKE).put(STROKE_KEY, strokeJson(stroke)).toString())
    }

    /** Replaces the server's page; used on connect, undo, clear and erase. */
    fun sendPage(strokes: List<SavedStroke>) {
        val array = JSONArray()
        strokes.forEach { array.put(strokeJson(it)) }
        enqueue(JSONObject().put(T, PAGE).put(STROKES, array).toString())
    }

    private fun strokeJson(stroke: SavedStroke): JSONObject {
        val samples = JSONArray()
        for (i in 0 until stroke.count * SAMPLE_STRIDE) samples.put(stroke.samples[i].toDouble())
        val times = JSONArray()
        for (i in 0 until stroke.count) times.put(stroke.times[i])
        return JSONObject()
            .put("preset", stroke.presetIndex)
            .put("argb", stroke.argb)
            .put("scale", stroke.sizeScale.toDouble())
            // Decimal string: JS numbers cannot hold a 64-bit seed exactly.
            .put("seed", stroke.seed.toString())
            .put("samples", samples)
            .put("times", times)
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
            override fun onOpen(
                webSocket: WebSocket,
                response: Response,
            ) {
                connecting.set(false)
                Log.i(TAG, "connected")
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
        const val NORMAL_CLOSURE = 1000
        const val SAMPLE_STRIDE = 5
        const val T = "t"
        const val STROKE = "stroke"
        const val PAGE = "page"
        const val STROKE_KEY = "stroke"
        const val STROKES = "strokes"
    }
}
