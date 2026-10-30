package dev.penwise.frames

import dev.penwise.ink.HitAnswer
import org.json.JSONException
import org.json.JSONObject

/** The answer to a still poll. */
internal sealed interface StillReply {
    /** A new manifest with the `ETag` to send next time. */
    class Fresh(
        val manifest: StillManifest,
        val etag: String?,
    ) : StillReply

    /** `304`: what the tablet has is current. */
    data object Unchanged : StillReply

    /** `503` + `Retry-After`: the server is making the still; ask again after [retryAfterMs]. The old still stays. */
    class Busy(
        val retryAfterMs: Long,
    ) : StillReply

    /** No still yet (`404`), or the server is away. */
    data object Missing : StillReply
}

/** The server's per-frame reads: the still manifest and strips, and the pen-down hit for points off the window. */
internal class StillClient(
    server: FrameServer,
) {
    private val http = FrameHttp(server)

    /** The pen-down hit comes from a hit map in the server's memory: it either answers fast or not at all. */
    private val hitHttp = FrameHttp(server, HIT_TIMEOUT_MS)

    /** `GET /frames/{id}/still` with `If-None-Match: [etag]`. */
    fun manifest(
        frameId: String,
        etag: String?,
    ): StillReply {
        val reply = http.get("/frames/$frameId/still", etag)
        return when {
            reply != null && reply.unavailable -> {
                StillReply.Busy(StillBackoff.retryAfterMs(reply.retryAfter))
            }

            reply == null || !(reply.ok || reply.notModified) -> {
                StillReply.Missing
            }

            reply.notModified -> {
                StillReply.Unchanged
            }

            else -> {
                StillManifests.parse(reply.body.decodeToString())?.let { StillReply.Fresh(it, reply.etag) }
                    ?: StillReply.Missing
            }
        }
    }

    /** The PNG bytes of one strip; [url] is as the manifest gave it (a server path or an absolute URL). */
    fun strip(url: String): ByteArray? = http.get(url)?.takeIf { it.ok }?.body

    /**
     * `GET /frames/{id}/hit?x&y` ([x], [y] in document CSS px): the element under the point. The rect is in document
     * coordinates (the bridge's is in viewport px), which [HitAnswer.document] says.
     */
    fun hit(
        frameId: String,
        x: Float,
        y: Float,
    ): HitAnswer? {
        val reply = hitHttp.get("/frames/$frameId/hit?x=$x&y=$y")?.takeIf { it.ok } ?: return null
        return parseHit(reply.body.decodeToString())
    }

    internal companion object {
        /** The hit payload (the bridge's `pw:hit` shape, rect in document px); null when nothing is under the point. */
        fun parseHit(json: String): HitAnswer? =
            try {
                val message = JSONObject(json)
                val key = message.optString("key")
                val rect = message.optJSONArray("rect")
                if (key.isEmpty() || rect == null || rect.length() < RECT_VALUES) {
                    null
                } else {
                    HitAnswer(key, List(RECT_VALUES) { rect.optDouble(it).toFloat() }, document = true)
                }
            } catch (ignored: JSONException) {
                null
            }

        private const val RECT_VALUES = 4
        private const val HIT_TIMEOUT_MS = 800
    }
}
