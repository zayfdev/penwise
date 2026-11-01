package dev.penwise.sync

import android.util.Log
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import okio.ByteString
import okio.ByteString.Companion.toByteString
import java.util.concurrent.ScheduledFuture
import java.util.concurrent.TimeUnit

private const val TAG = "PenwiseSync"
private const val NORMAL_CLOSURE = 1000
private const val PING_SECONDS = 20L
private const val QUIET_CATCH_UP_MS = 800L

/** Where the socket stands. */
enum class SyncStatus { STOPPED, CONNECTING, ONLINE, BAD_TOKEN }

/** What a [SyncConnection] reports; every callback runs on the sync thread or an OkHttp thread. */
interface SyncListener {
    /** A server frame was applied to the core. */
    fun onOutcome(outcome: Outcome)

    /** The status changed. */
    fun onStatus(status: SyncStatus)

    /**
     * The replica holds everything the server announced at `Welcome` (the stream also went quiet). Fires once per
     * connection; the outbox may still be flushing.
     */
    fun onCaughtUp()

    /** The first connection attempt of this run failed (no server reachable). */
    fun onUnreachable()

    /** The state file cannot be written (`true`, e.g. disk full) or writes work again (`false`). */
    fun onStateFailing(failing: Boolean) {}
}

/**
 * The `/sync` WebSocket: moves binary frames between the socket and a [SyncCore] and nothing else.
 *
 * `Hello` goes out on open; every received frame goes through [SyncCore.receive], its replies and the next
 * `PushOps` are sent back; a closed or failed socket reconnects with growing delay, trying the endpoints in turn.
 * The unacknowledged outbox lives in the core (and its state file), so reconnecting resends whatever was not
 * acknowledged; nothing is queued here. Live previews are droppable and sent only while online.
 */
class SyncConnection(
    private val core: SyncCore,
    private val endpoints: () -> List<String>,
    private val token: () -> String,
    private val listener: SyncListener,
    private val clock: () -> Long = System::currentTimeMillis,
    private val activeNetwork: () -> Any? = { null },
    private val client: OkHttpClient =
        OkHttpClient
            .Builder()
            .pingInterval(PING_SECONDS, TimeUnit.SECONDS)
            .build(),
) {
    private val thread = SyncThread()
    private val backoff = Backoff()

    // The fields below are touched only on the sync thread, except `socket` and `status`, which are volatile.
    @Volatile
    private var socket: WebSocket? = null

    @Volatile
    var status: SyncStatus = SyncStatus.STOPPED
        private set(next) {
            if (field == next) return
            field = next
            listener.onStatus(next)
        }
    private var running = false
    private val firstContact = FirstContact(endpoints, listener::onUnreachable)
    private val quiet: QuietTimer =
        QuietTimer(thread, QUIET_CATCH_UP_MS) {
            // Quiet only counts as "caught up" when the replica holds what `Welcome` announced; a slow stream waits.
            if (core.caughtUp()) listener.onCaughtUp() else quiet.rearm()
        }
    private var reconnect: ScheduledFuture<*>? = null
    private val networkRetry = NetworkRetry(clock)
    private var networkLook: ScheduledFuture<*>? = null

    /** The network that was usable when the current socket was opened; null when none was known yet. */
    private var openedOn: Any? = null

    /** The endpoint the socket is open to (or being opened to); `null` while stopped. */
    @Volatile
    var currentUrl: String? = null
        private set

    /**
     * Starts (or restarts) connecting, at the endpoint that last worked. After a settings change pass
     * [forgetGood]: the endpoint list changed, so its first entry (the new override) is tried first.
     */
    fun start(forgetGood: Boolean = false) {
        thread.run {
            running = true
            reconnect?.cancel(false)
            closeSocket()
            backoff.restart(forgetGood)
            open()
        }
    }

    /**
     * The device's default network became usable ([network] identifies it). Looks at the socket after a short
     * debounce (a newer event replaces the pending look) and redials at once if it is waiting out its backoff, or
     * is online on another network. A dial in flight or a healthy socket is left alone.
     */
    fun networkValidated(network: Any) {
        thread.run {
            val delay = networkRetry.onValidated(network)
            networkLook?.cancel(false)
            networkLook =
                thread.later(delay, TimeUnit.MILLISECONDS) {
                    val online = status == SyncStatus.ONLINE
                    if (online && openedOn == null) openedOn = networkRetry.latest
                    if (running && networkRetry.shouldRedial(socket != null, online, openedOn)) {
                        Log.i(TAG, "network is back; reconnecting now")
                        networkRetry.redialed()
                        reconnect?.cancel(false)
                        closeSocket()
                        backoff.restart()
                        open()
                    }
                }
        }
    }

    /** Stops for good: closes the socket and cancels retries. */
    fun stop() {
        thread.run {
            running = false
            reconnect?.cancel(false)
            networkLook?.cancel(false)
            closeSocket()
            status = SyncStatus.STOPPED
        }
        thread.shutdown()
    }

    /** Sends the next `PushOps` frame if the session allows one; call after local commits. */
    fun kick() = thread.run { socket?.let(::flushPushes) }

    /** Sends a droppable preview frame; ignored while offline. */
    fun sendLive(frame: ByteArray) {
        val link = socket
        if (status == SyncStatus.ONLINE && link != null) link.send(frame.toByteString())
    }

    private fun open() {
        if (!running || socket != null) return
        val urls = endpoints()
        if (urls.isEmpty()) {
            status = SyncStatus.STOPPED
            firstContact.failed(null)
            return
        }
        val url = urls[backoff.endpoint(urls)]
        currentUrl = url
        openedOn = null
        Log.i(TAG, "connecting to $url")
        status = SyncStatus.CONNECTING
        socket = client.newWebSocket(Request.Builder().url(url).build(), Link())
    }

    private fun closeSocket() {
        quiet.cancel()
        socket?.close(NORMAL_CLOSURE, "closing")
        socket = null
        core.offline()
    }

    private fun scheduleReconnect() {
        if (running) reconnect = backoff.schedule(thread, ::open)
    }

    private fun flushPushes(link: WebSocket) {
        while (true) {
            val frame = core.pollPush() ?: return
            if (!link.send(frame.toByteString())) return
        }
    }

    private fun handle(
        link: WebSocket,
        frame: ByteArray,
    ) {
        if (link !== socket) return
        val outcome = core.receive(frame)
        outcome.replies.forEach { link.send(it.toByteString()) }
        when {
            outcome.kind == Kind.WELCOME -> {
                currentUrl?.let(backoff::welcomed)
                firstContact.welcomed()
                status = SyncStatus.ONLINE
                quiet.rearm()
            }

            outcome.kind == Kind.REJECT -> {
                Log.w(TAG, "server rejected the handshake (code ${outcome.code})")
                if (outcome.code == Kind.REJECT_BAD_TOKEN) status = SyncStatus.BAD_TOKEN
                closeSocket()
                firstContact.failed(currentUrl)
                scheduleReconnect()
            }

            outcome.kind in Kind.CATCH_UP -> {
                quiet.rearm()
            }
        }
        flushPushes(link)
        listener.onOutcome(outcome)
        if (outcome.reconnect) {
            // A different server: the replica was reset; say hello again right away.
            closeSocket()
            backoff.restart()
            open()
        }
    }

    private fun lost(
        link: WebSocket,
        why: String,
    ) {
        if (link !== socket) return
        Log.i(TAG, "sync socket closed: $why")
        socket = null
        quiet.cancel()
        core.offline()
        firstContact.failed(currentUrl)
        if (status != SyncStatus.BAD_TOKEN) status = SyncStatus.CONNECTING
        scheduleReconnect()
    }

    private inner class Link : WebSocketListener() {
        override fun onOpen(
            webSocket: WebSocket,
            response: Response,
        ) {
            thread.run {
                if (webSocket === socket) {
                    // The network the socket really opened on (a dial may land on another one than it started on).
                    openedOn = activeNetwork() ?: networkRetry.latest
                    webSocket.send(core.hello(token()).toByteString())
                }
            }
        }

        override fun onMessage(
            webSocket: WebSocket,
            bytes: ByteString,
        ) {
            val frame = bytes.toByteArray()
            thread.run { handle(webSocket, frame) }
        }

        override fun onMessage(
            webSocket: WebSocket,
            text: String,
        ) {
            Log.w(TAG, "ignoring a text frame (the protocol is binary)")
        }

        override fun onClosed(
            webSocket: WebSocket,
            code: Int,
            reason: String,
        ) = thread.run { lost(webSocket, "$code $reason") }

        override fun onFailure(
            webSocket: WebSocket,
            t: Throwable,
            response: Response?,
        ) = thread.run { lost(webSocket, t.message.orEmpty()) }
    }
}
