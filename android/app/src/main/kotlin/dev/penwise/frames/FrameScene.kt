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
    /** `full`: the box is the document (design harness v2); `viewport` is the v1 scrolling window. */
    val full: Boolean = false,
    /** The server-measured document height in CSS px (0 = unknown). */
    val docHeight: Int = 0,
    /** The box height on the board (the core's `boxH`): `max(css_h, doc_h)` for a full frame, else `css_h`. */
    val boxHeight: Int = cssHeight,
    /** The ghost/variant patch (P1) as its JSON text, or null when the frame has none. */
    val patch: String? = null,
    /** The variant label (`""` when none). */
    val label: String = "",
    /** The variant group id, or null for a frame the owner placed themselves. */
    val group: String? = null,
    /** The owner picked this variant. */
    val chosen: Boolean = false,
)

/**
 * The core's frame list (`sceneFrames`), parsed:
 * `[{"id","hex","project","route","preset","cssW","cssH","extent","docH","boxH","rect":[x,y,w,h],"patch":{…}|null,
 * "label","group","chosen"}]`, in paint order. `hex` is the 32-hex id the HTTP routes take; `extent` is `full` or
 * `viewport`; `docH` the server-measured document height (0 = unknown); `boxH` the box height on the board as the
 * core computes it from the frame's layout (the client never does). Pure JSON through `org.json`, no Android types,
 * so the parse is unit-tested.
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
        // `hex` is the form the HTTP routes take; the core sends it so the client never converts an id itself.
        val id = frame.optString(HEX).ifEmpty { frame.optString(ID) }
        if (id.length != ID_LENGTH) return null
        val rect = frame.optJSONArray(RECT)
        val cssHeight = frame.optInt(CSS_H, FramePreset.PHONE.cssHeight)
        return SceneFrame(
            id = id,
            project = frame.optString(PROJECT),
            route = frame.optString(ROUTE, "/"),
            preset = frame.optString(PRESET, FramePreset.PHONE.key),
            cssWidth = frame.optInt(CSS_W, FramePreset.PHONE.cssWidth),
            cssHeight = cssHeight,
            x = rect?.optDouble(0)?.toFloat() ?: 0f,
            y = rect?.optDouble(1)?.toFloat() ?: 0f,
            full = frame.optString(EXTENT) == FULL,
            docHeight = frame.optInt(DOC_H, 0),
            boxHeight = frame.optInt(BOX_H, 0).takeIf { it > 0 } ?: cssHeight,
            patch = patch(frame.opt(PATCH)),
            label = string(frame.opt(LABEL)),
            group = string(frame.opt(GROUP)).takeIf { it.isNotEmpty() },
            chosen = frame.optBoolean(CHOSEN, false),
        )
    }

    /** The patch as the JSON text the bridge is sent (the core sends an object; text is accepted too). */
    private fun patch(value: Any?): String? =
        when (value) {
            is JSONObject -> value.toString()
            is String -> value.takeIf { it.isNotEmpty() }
            else -> null
        }

    /** A string the core sent; `null` and non-strings read as empty. */
    private fun string(value: Any?): String = value as? String ?: ""

    private const val ID = "id"
    private const val HEX = "hex"
    private const val PROJECT = "project"
    private const val ROUTE = "route"
    private const val PRESET = "preset"
    private const val CSS_W = "cssW"
    private const val CSS_H = "cssH"
    private const val EXTENT = "extent"
    private const val FULL = "full"
    private const val DOC_H = "docH"
    private const val BOX_H = "boxH"
    private const val RECT = "rect"
    private const val PATCH = "patch"
    private const val LABEL = "label"
    private const val GROUP = "group"
    private const val CHOSEN = "chosen"

    /** A hex item id: 16 bytes. */
    private const val ID_LENGTH = 32
}
