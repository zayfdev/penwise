package dev.penwise.frames

import org.json.JSONArray
import org.json.JSONException
import org.json.JSONObject

/** One frame item of a page, as the core reports it (`sceneFrames`): the page's copy, before a URL is known. */
data class SceneFrame(
    val id: String,
    val project: String,
    val route: String,
    val preset: String,
    val cssWidth: Int,
    val cssHeight: Int,
    val x: Float,
    val y: Float,
)

/**
 * The core's frame list (`sceneFrames`), parsed: `[{"id","project","route","preset","cssW","cssH","rect":[x,y,w,h]}]`,
 * in paint order. Pure JSON through `org.json`, no Android types, so the parse is unit-tested.
 */
internal object SceneFrames {
    /** The frames in [json]; empty for anything the core would not send (a malformed array, a frame without an id). */
    fun parse(json: String): List<SceneFrame> =
        try {
            val array = JSONArray(json)
            (0 until array.length()).mapNotNull { array.optJSONObject(it)?.let(::frame) }
        } catch (ignored: JSONException) {
            emptyList()
        }

    private fun frame(frame: JSONObject): SceneFrame? {
        val id = frame.optString(ID)
        if (id.length != ID_LENGTH) return null
        val rect = frame.optJSONArray(RECT)
        return SceneFrame(
            id = id,
            project = frame.optString(PROJECT),
            route = frame.optString(ROUTE, "/"),
            preset = frame.optString(PRESET, FramePreset.PHONE.key),
            cssWidth = frame.optInt(CSS_W, FramePreset.PHONE.cssWidth),
            cssHeight = frame.optInt(CSS_H, FramePreset.PHONE.cssHeight),
            x = rect?.optDouble(0)?.toFloat() ?: 0f,
            y = rect?.optDouble(1)?.toFloat() ?: 0f,
        )
    }

    private const val ID = "id"
    private const val PROJECT = "project"
    private const val ROUTE = "route"
    private const val PRESET = "preset"
    private const val CSS_W = "cssW"
    private const val CSS_H = "cssH"
    private const val RECT = "rect"

    /** A hex item id: 16 bytes. */
    private const val ID_LENGTH = 32
}
