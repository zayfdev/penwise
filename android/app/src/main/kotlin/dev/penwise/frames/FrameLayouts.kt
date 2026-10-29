package dev.penwise.frames

import org.json.JSONArray
import org.json.JSONException
import org.json.JSONObject

/**
 * The core's frame-layout wire format (K1): the `pw:layout` message with the frame's preset, and the JSON array
 * `sceneFrameKeys` returns. Pure `org.json`, no Android types, so both are unit-tested.
 */
internal object FrameLayouts {
    /** `{"preset","scroll":[x,y],"doc":[w,h],"viewport":[w,h],"state","rects":{key:[x,y,w,h]|null}}`. */
    fun coreJson(
        layout: FrameMessage.Layout,
        preset: String,
    ): String {
        val rects = JSONObject()
        for ((key, rect) in layout.rects) {
            val value =
                if (rect == null) {
                    JSONObject.NULL
                } else {
                    JSONArray()
                        .put(rect.x.toDouble())
                        .put(rect.y.toDouble())
                        .put(rect.width.toDouble())
                        .put(rect.height.toDouble())
                }
            rects.put(key, value)
        }
        return JSONObject()
            .put("preset", preset)
            .put("scroll", values(layout.scroll))
            .put("doc", values(layout.doc))
            .put("viewport", values(layout.viewport))
            .put("state", layout.state)
            .put("rects", rects)
            .toString()
    }

    /** The keys of a `sceneFrameKeys` JSON array; empty for anything the core would not send. */
    fun keys(json: String): List<String> =
        try {
            val array = JSONArray(json)
            (0 until array.length()).mapNotNull { index ->
                array.optString(index).takeIf { it.isNotEmpty() }
            }
        } catch (ignored: JSONException) {
            emptyList()
        }

    private fun values(list: List<Float>): JSONArray {
        val array = JSONArray()
        for (value in list) array.put(value.toDouble())
        return array
    }
}
