package dev.penwise.frames

import dev.penwise.ink.ViewTransform
import org.json.JSONArray
import org.json.JSONException
import org.json.JSONObject
import kotlin.math.max
import kotlin.math.min

/** A mark's status (K1): new, seen by the agent, done (hidden unless show-done), or reopened after a done. */
enum class MarkStatus(
    val wire: String,
) {
    NEW("new"),
    SEEN("seen"),
    DONE("done"),
    REOPENED("reopened"),
    ;

    companion object {
        fun from(wire: String): MarkStatus = entries.firstOrNull { it.wire == wire } ?: NEW
    }
}

/** One mark as the core reports it (`sceneMarks`): its frame, preset, status, state and effective page rect. */
data class SceneMark(
    val id: String,
    val frame: String,
    val preset: String,
    val status: MarkStatus,
    val state: String,
    val strokes: List<String>,
    val rect: FrameRect?,
)

/** A mark's chip on screen, already placed: the page point becomes screen px through the canvas transform. */
data class MarkChip(
    val id: String,
    val status: MarkStatus,
    val state: String,
    val frame: String,
    val dimmed: Boolean,
    val left: Float,
    val top: Float,
)

/** `sceneMarks`' JSON and where each chip goes: text and geometry only, so both are unit-tested. */
internal object FrameMarks {
    /** The marks in [json]; empty for malformed input or an id that is not 32 hex. */
    fun parse(json: String): List<SceneMark> =
        try {
            val array = JSONArray(json)
            (0 until array.length()).mapNotNull { array.optJSONObject(it)?.let(::mark) }
        } catch (ignored: JSONException) {
            emptyList()
        }

    private fun mark(objects: JSONObject): SceneMark? {
        val id = objects.optString("id")
        val frame = objects.optString("frame")
        if (!isId(id) || !isId(frame)) return null
        val rect =
            objects.optJSONArray("rect")?.let { values ->
                if (values.length() < RECT_VALUES) {
                    null
                } else {
                    FrameRect(
                        values.optDouble(0).toFloat(),
                        values.optDouble(1).toFloat(),
                        values.optDouble(2).toFloat(),
                        values.optDouble(3).toFloat(),
                    )
                }
            }
        return SceneMark(
            id = id,
            frame = frame,
            preset = objects.optString("preset"),
            status = MarkStatus.from(objects.optString("status")),
            state = objects.optString("state"),
            strokes = strings(objects.optJSONArray("strokes")),
            rect = rect,
        )
    }

    private fun strings(array: JSONArray?): List<String> {
        if (array == null) return emptyList()
        return (0 until array.length()).mapNotNull { array.optString(it).takeIf(String::isNotEmpty) }
    }

    private fun isId(text: String): Boolean = text.length == ID_LENGTH

    private const val RECT_VALUES = 4

    /** A hex item id: 16 bytes. */
    private const val ID_LENGTH = 32
}

/**
 * Where the chips go: the top-right corner of the mark's rect, clipped to its frame (a mark outside the frame has no
 * chip), dimmed when its state recipe is not the frame's current one (the core already drops hidden marks' rects).
 */
internal object MarkPlacement {
    fun chips(
        marks: List<SceneMark>,
        frames: List<FrameSpec>,
        states: Map<String, String>,
        view: ViewTransform,
    ): List<MarkChip> {
        val byId = frames.associateBy { it.id }
        return marks.mapNotNull { chip(it, byId[it.frame], states, view) }
    }

    /** One mark's chip, or null when the core hid it or nothing of it falls inside its frame. */
    private fun chip(
        mark: SceneMark,
        frame: FrameSpec?,
        states: Map<String, String>,
        view: ViewTransform,
    ): MarkChip? {
        if (frame == null) return null
        val rect = mark.rect ?: return null
        val left = max(rect.x, frame.x)
        val top = max(rect.y, frame.y)
        val right = min(rect.x + rect.width, frame.x + frame.cssWidth)
        val bottom = min(rect.y + rect.height, frame.y + frame.cssHeight)
        if (right <= left || bottom <= top) return null
        val dimmed = mark.state.isNotEmpty() && mark.state != states[mark.frame].orEmpty()
        return MarkChip(
            id = mark.id,
            status = mark.status,
            state = mark.state,
            frame = mark.frame,
            dimmed = dimmed,
            left = view.toScreenX(right),
            top = view.toScreenY(top),
        )
    }
}
