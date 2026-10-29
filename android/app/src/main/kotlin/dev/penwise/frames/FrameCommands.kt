package dev.penwise.frames

import android.webkit.WebView
import dev.penwise.ink.HitAnswer
import org.json.JSONArray
import org.json.JSONObject

/**
 * What the tablet sends into a frame's page (K2). An Android WebView has no parent window, so the host calls the
 * bridge's one entry point directly; the script is JSON-escaped by [JSONObject]'s `toString`.
 */
internal object FrameCommands {
    fun watch(
        view: WebView,
        keys: List<String>,
    ) {
        val array = JSONArray()
        for (key in keys) array.put(key)
        post(view, JSONObject().put("t", "pw:watch").put("keys", array))
    }

    /** [x]/[y] are the page's CSS viewport px, the space the bridge reports rects in. */
    fun hit(
        view: WebView,
        id: Int,
        x: Float,
        y: Float,
    ) {
        post(
            view,
            JSONObject()
                .put("t", "pw:hit?")
                .put("id", id)
                .put("x", x.toDouble())
                .put("y", y.toDouble()),
        )
    }

    fun replay(
        view: WebView,
        state: String,
    ) {
        post(view, JSONObject().put("t", "pw:replay").put("state", state))
    }

    /** The ghost/variant patch decision (P3), as [PatchFeed] built it: `{t:"pw:patch", patch, on}`. */
    fun patch(
        view: WebView,
        message: JSONObject,
    ) {
        post(view, message)
    }

    private fun post(
        view: WebView,
        message: JSONObject,
    ) {
        view.evaluateJavascript("window.PenwiseBridge&&window.PenwiseBridge.receive($message)", null)
    }
}

/**
 * The element hit requests in flight: a pen-down asks one, the bridge's `pw:hit` answers it, and the commit takes
 * whatever arrived by pen-up (nothing = area binding). Ids are unique per request, so a late answer cannot bind
 * the wrong stroke.
 */
internal class FrameHits {
    private val pending = HashSet<Int>()
    private val answered = HashMap<Int, HitAnswer>()
    private var next = 1

    /** Asks the page what is under ([x], [y]) CSS px; the answer is taken with [take]. */
    fun ask(
        view: WebView,
        x: Float,
        y: Float,
    ): Int {
        val id = next++
        pending += id
        FrameCommands.hit(view, id, x, y)
        return id
    }

    fun answer(
        id: Int,
        key: String,
        rect: List<Float>,
    ) {
        if (pending.remove(id)) answered[id] = HitAnswer(key, rect)
    }

    /** The element under the point, or null when the page had nothing there (or has not answered). */
    fun take(id: Int): HitAnswer? {
        pending -= id
        return answered.remove(id)?.takeIf { it.key.isNotEmpty() }
    }

    fun forget(id: Int) {
        pending -= id
        answered.remove(id)
    }

    fun clear() {
        pending.clear()
        answered.clear()
    }
}
