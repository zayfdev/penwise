package dev.penwise.frames

import android.webkit.JavascriptInterface
import org.json.JSONArray
import org.json.JSONException
import org.json.JSONObject

/**
 * One message the injected bridge (`/__penwise/bridge.js`) sent from inside a frame. M2 adds patches, element key
 * updates and state; those are not parsed yet.
 */
sealed interface FrameMessage {
    /** `pw:hello`, on every load: the document the frame shows and the viewport it is laid out in. */
    data class Hello(
        val url: String,
        val docWidth: Int,
        val docHeight: Int,
        val viewWidth: Int,
        val viewHeight: Int,
    ) : FrameMessage

    /** `pw:scroll`, throttled to a frame: the page's scroll offset and total size. */
    data class Scroll(
        val x: Int,
        val y: Int,
        val docWidth: Int,
        val docHeight: Int,
    ) : FrameMessage

    /** `pw:hit`, the answer to a `pw:hit?`: what element is under a point. */
    data class Hit(
        val id: Int,
        val key: String,
        val selector: String,
        val rect: List<Float>,
    ) : FrameMessage
}

/**
 * The bridge wire format, page → host. Pure JSON through `org.json`, no Android types: the parse is unit-tested. The
 * host → page direction (`pw:hit?`, `pw:scrollTo`) is not needed before M2 and is not built yet.
 */
internal object FrameMessages {
    /** The object name the injected bridge looks for in a WebView (`window.PenwiseHost.post(json)`). */
    const val HOST = "PenwiseHost"

    /** Decodes one message; null for a type this client does not know or a payload that is not JSON. */
    fun parse(json: String): FrameMessage? =
        try {
            decode(JSONObject(json))
        } catch (ignored: JSONException) {
            null
        }

    private fun decode(message: JSONObject): FrameMessage? =
        when (message.optString(TYPE)) {
            "pw:hello" -> hello(message)
            "pw:scroll" -> scroll(message)
            "pw:hit" -> hit(message)
            else -> null
        }

    private fun hello(message: JSONObject) =
        FrameMessage.Hello(
            url = message.optString("url"),
            docWidth = message.optInt("docW"),
            docHeight = message.optInt("docH"),
            viewWidth = message.optInt("vw"),
            viewHeight = message.optInt("vh"),
        )

    private fun scroll(message: JSONObject) =
        FrameMessage.Scroll(
            x = message.optInt("x"),
            y = message.optInt("y"),
            docWidth = message.optInt("docW"),
            docHeight = message.optInt("docH"),
        )

    private fun hit(message: JSONObject) =
        FrameMessage.Hit(
            id = message.optInt("id", -1),
            key = message.optString("key"),
            selector = message.optString("selector"),
            rect = rect(message.optJSONArray("rect")),
        )

    private fun rect(array: JSONArray?): List<Float> {
        if (array == null) return emptyList()
        return (0 until array.length()).map { array.optDouble(it).toFloat() }
    }

    private const val TYPE = "t"
}

/**
 * The `PenwiseHost` object inside a frame's WebView: what the injected bridge calls with `window.PenwiseHost.post`.
 * The WebView calls this on its own thread, so [sink] must marshal before touching views or state.
 */
internal class FrameBridge(
    private val sink: (FrameMessage) -> Unit,
) {
    @JavascriptInterface
    fun post(json: String) {
        FrameMessages.parse(json)?.let(sink)
    }
}
