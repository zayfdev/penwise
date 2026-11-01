package dev.penwise.frames

import android.util.Log
import java.io.IOException
import java.net.HttpURLConnection
import java.net.URL

/** The PC server a frame talks to over plain HTTP: the base URL the tablet syncs with, and its token. */
data class FrameServer(
    val base: String,
    val token: String,
)

/**
 * A blocking HTTP call to the PC server (stills, hits, rounds): the base URL, the token header, an optional
 * `If-None-Match`. Best effort, so callers run it off the main thread and treat `null` as "server away".
 */
internal class FrameHttp(
    private val server: FrameServer,
    private val timeoutMs: Int = TIMEOUT_MS,
) {
    /** A response: the status [code], the [body] and the `ETag` header when there is one. */
    class Reply(
        val code: Int,
        val body: ByteArray,
        val etag: String?,
        /** The `Retry-After` header as sent (seconds), or null. */
        val retryAfter: String? = null,
    ) {
        val ok: Boolean get() = code in HTTP_OK_FIRST..HTTP_OK_LAST
        val notModified: Boolean get() = code == HTTP_NOT_MODIFIED
        val unavailable: Boolean get() = code == HTTP_UNAVAILABLE
    }

    /** `GET` of a server path or an absolute URL. */
    fun get(
        pathOrUrl: String,
        etag: String? = null,
    ): Reply? = call("GET", pathOrUrl, etag)

    /** `POST` with an empty body. */
    fun post(path: String): Reply? = call("POST", path, null)

    /** `PUT` of a JSON [body]. */
    fun put(
        path: String,
        body: String,
    ): Reply? = call("PUT", path, null, body.toByteArray(Charsets.UTF_8))

    private fun call(
        method: String,
        pathOrUrl: String,
        etag: String?,
        body: ByteArray? = null,
    ): Reply? {
        val url = if (pathOrUrl.startsWith("http")) pathOrUrl else server.base.trimEnd('/') + pathOrUrl
        val connection = runCatching { URL(url).openConnection() as HttpURLConnection }.getOrNull() ?: return null
        return try {
            connection.requestMethod = method
            connection.connectTimeout = timeoutMs
            connection.readTimeout = timeoutMs
            if (server.token.isNotEmpty()) connection.setRequestProperty("X-Penwise-Token", server.token)
            if (etag != null) connection.setRequestProperty("If-None-Match", etag)
            if (method == "POST" || body != null) {
                connection.doOutput = true
                connection.setFixedLengthStreamingMode(body?.size ?: 0)
                if (body != null) {
                    connection.setRequestProperty("Content-Type", "application/json")
                    connection.outputStream.use { it.write(body) }
                }
            }
            val code = connection.responseCode
            val stream = if (code in HTTP_OK_FIRST..HTTP_OK_LAST) connection.inputStream else connection.errorStream
            val body = stream?.use { it.readBytes() } ?: ByteArray(0)
            Reply(code, body, connection.getHeaderField("ETag"), connection.getHeaderField("Retry-After"))
        } catch (error: IOException) {
            Log.w(TAG, "$method $pathOrUrl failed: ${error.message}")
            null
        } finally {
            connection.disconnect()
        }
    }

    internal companion object {
        private const val TAG = "PenwiseFrames"

        /** The default patience for a poll or a send. */
        const val TIMEOUT_MS = 4_000
        private const val HTTP_OK_FIRST = 200
        private const val HTTP_OK_LAST = 299
        private const val HTTP_NOT_MODIFIED = 304
        private const val HTTP_UNAVAILABLE = 503
    }
}

/** What the Send button's call came back with. */
enum class SendResult {
    /** The round was sent. */
    SENT,

    /** `409`: no new marks since the last round. */
    NOTHING_TO_SEND,

    /** The server was away or refused. */
    FAILED,
}

/** `POST /projects/{project}/rounds/send` (design harness v2 §4). */
internal object RoundsClient {
    fun send(
        server: FrameServer,
        project: String,
    ): SendResult {
        val reply = FrameHttp(server).post("/projects/${java.net.URLEncoder.encode(project, "UTF-8")}/rounds/send")
        return when {
            reply == null -> SendResult.FAILED
            reply.ok -> SendResult.SENT
            reply.code == HTTP_CONFLICT -> SendResult.NOTHING_TO_SEND
            else -> SendResult.FAILED
        }
    }

    private const val HTTP_CONFLICT = 409
}
