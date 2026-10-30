package dev.penwise.frames

import android.content.Context
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import dev.penwise.ink.HitAnswer
import java.io.File
import java.util.concurrent.ExecutorService
import java.util.concurrent.Executors
import java.util.concurrent.RejectedExecutionException

/**
 * Keeps the frames' stills current (contract §4): every [POLL_MS] the visible frames are asked for their manifest with
 * `If-None-Match`, so an unchanged still costs a `304`; a new version keeps the strips it shares with the old one and
 * is announced. Network and decoding run on worker threads (pen-down hits on a thread of their own, so a poll never
 * delays one); every callback is on the main thread. The poll runs only between [start] and [stop].
 */
internal class StillFeed(
    context: Context,
    private val onManifest: (frameId: String, manifest: StillManifest) -> Unit,
    private val onBitmap: () -> Unit,
) {
    private val main = Handler(Looper.getMainLooper())
    private val pool: ExecutorService = Executors.newFixedThreadPool(POLL_THREADS) { Thread(it, "penwise-still-poll") }
    private val hitPool: ExecutorService = Executors.newSingleThreadExecutor { Thread(it, "penwise-hit") }
    private val etags = HashMap<String, String?>()
    private val polledAt = HashMap<String, Long>()
    private val busy = HashSet<String>()
    private val backoff = StillBackoff()
    private var visible: () -> List<String> = { emptyList() }
    private var released = false
    private val loop =
        PollLoop(POLL_MS, { run, delay -> main.postDelayed(run, delay) }, main::removeCallbacks) { refresh() }

    @Volatile
    private var client: StillClient? = null

    /** The server the stills come from; null while the tablet knows none (placeholders only). */
    var server: FrameServer? = null
        set(value) {
            if (field == value) return
            field = value
            client = value?.let { StillClient(it) }
            etags.clear()
            polledAt.clear()
        }

    /** The manifests received so far, by frame id. */
    val manifests = HashMap<String, StillManifest>()

    val bitmaps = StillBitmaps(File(context.cacheDir, "stills"), { client }, { main.post(onBitmap) })

    /** Starts polling; [visible] says which frames are on screen at each tick. */
    fun start(visible: () -> List<String>) {
        this.visible = visible
        resume()
    }

    /** Polls again after a [stop] (the app is back in front). */
    fun resume() {
        if (!released) loop.start()
    }

    /** Stops polling (the app is in the background); what is on screen stays. */
    fun stop() = loop.stop()

    /** Asks now for the visible frames that have no still yet or have not been asked for a while. */
    fun refresh(force: Boolean = false) {
        val now = SystemClock.uptimeMillis()
        for (id in visible()) {
            if (id in busy || (!force && backoff.blocked(id, now))) continue
            val last = polledAt[id] ?: 0L
            if (force || id !in manifests || now - last >= POLL_MS) fetch(id, now)
        }
    }

    /** Asks the server what is under document point ([x], [y]) of [frameId]; [done] runs on the main thread. */
    fun hit(
        frameId: String,
        x: Float,
        y: Float,
        done: (HitAnswer?) -> Unit,
    ) {
        val http = client
        val queued =
            http != null &&
                submit(hitPool) {
                    val answer = http.hit(frameId, x, y)
                    main.post { done(answer) }
                }
        if (!queued) done(null)
    }

    /** Forgets a frame that is gone. */
    fun forget(frameId: String) {
        manifests.remove(frameId)
        etags.remove(frameId)
        polledAt.remove(frameId)
        backoff.clear(frameId)
        bitmaps.retain(frameId, emptyList())
    }

    fun release() {
        released = true
        loop.stop()
        pool.shutdownNow()
        hitPool.shutdownNow()
        bitmaps.release()
    }

    private fun fetch(
        id: String,
        now: Long,
    ) {
        val http = client ?: return
        busy += id
        polledAt[id] = now
        val etag = etags[id]
        val queued =
            submit(pool) {
                val reply = http.manifest(id, etag)
                main.post {
                    busy -= id
                    if (reply is StillReply.Busy) backoff.busy(id, SystemClock.uptimeMillis(), reply.retryAfterMs)
                    if (reply is StillReply.Fresh && !released) accept(id, reply)
                }
            }
        if (!queued) busy -= id
    }

    private fun accept(
        id: String,
        reply: StillReply.Fresh,
    ) {
        etags[id] = reply.etag
        manifests[id] = reply.manifest
        bitmaps.retain(id, reply.manifest.strips.map { it.url })
        onManifest(id, reply.manifest)
    }

    /** Runs [job] on [executor] unless it is shut down; false when it was not accepted. */
    private fun submit(
        executor: ExecutorService,
        job: () -> Unit,
    ): Boolean {
        if (executor.isShutdown) return false
        return try {
            executor.execute(job)
            true
        } catch (_: RejectedExecutionException) {
            false
        }
    }

    private companion object {
        const val POLL_MS = 3_000L
        const val POLL_THREADS = 2
    }
}
