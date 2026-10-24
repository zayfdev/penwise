package dev.penwise.ink

import org.json.JSONArray
import org.json.JSONObject

/**
 * Frames the tablet sends to the PC server. Serialization only; [SyncClient] owns the connection.
 *
 * The seed travels as a decimal string because a JS number cannot hold a 64-bit value exactly.
 */
internal object Wire {
    const val SAMPLE_STRIDE = 5

    fun stroke(stroke: SavedStroke): JSONObject {
        val samples = JSONArray()
        for (i in 0 until stroke.count * SAMPLE_STRIDE) samples.put(stroke.samples[i].toDouble())
        return JSONObject()
            .put("preset", stroke.presetIndex)
            .put("argb", stroke.argb)
            .put("scale", stroke.sizeScale.toDouble())
            .put("seed", stroke.seed.toString())
            .put("samples", samples)
            .put("times", times(stroke.times, stroke.count))
    }

    fun page(
        strokes: List<SavedStroke>,
        page: String,
    ): String {
        val array = JSONArray()
        strokes.forEach { array.put(stroke(it)) }
        return scoped(JSONObject().put("t", "page").put("strokes", array), page).toString()
    }

    fun strokeFrame(
        stroke: SavedStroke,
        page: String,
    ): String = scoped(JSONObject().put("t", "stroke").put("stroke", stroke(stroke)), page).toString()

    /** `{"t":"op"}` carrying one small edit, so the mirror does not resend the page. */
    fun removeOp(
        seeds: List<Long>,
        page: String,
    ): String {
        val array = JSONArray()
        seeds.forEach { array.put(it.toString()) }
        return scoped(
            JSONObject().put("t", "op").put("op", JSONObject().put("k", "remove").put("seeds", array)),
            page,
        ).toString()
    }

    /** Clears the page's strokes. */
    fun clearOp(page: String): String =
        scoped(JSONObject().put("t", "op").put("op", JSONObject().put("k", "clear")), page).toString()

    /** Replaces strokes with fragments (precise erase). */
    fun replaceOp(
        seeds: List<Long>,
        strokes: List<SavedStroke>,
        page: String,
    ): String {
        val seedArray = JSONArray()
        seeds.forEach { seedArray.put(it.toString()) }
        val strokeArray = JSONArray()
        strokes.forEach { strokeArray.put(stroke(it)) }
        val op = JSONObject().put("k", "replace").put("seeds", seedArray).put("strokes", strokeArray)
        return scoped(JSONObject().put("t", "op").put("op", op), page).toString()
    }

    /** Whole-set shape preview: each frame replaces the previous one, because a drag can shrink. */
    fun shapePreview(
        id: Int,
        preset: Int,
        argb: Int,
        scale: Float,
        seed: Long,
        samples: FloatArray,
        times: IntArray,
        count: Int,
        page: String,
    ): String {
        val floats = JSONArray()
        for (i in 0 until count * SAMPLE_STRIDE) floats.put(samples[i].toDouble())
        return scoped(
            JSONObject()
                .put("t", "live")
                .put("kind", "shape")
                .put("id", id)
                .put("preset", preset)
                .put("argb", argb)
                .put("scale", scale.toDouble())
                .put("seed", seed.toString())
                .put("samples", floats)
                .put("times", times(times, count)),
            page,
        ).toString()
    }

    /** One erase-cursor position (radius in page px). */
    fun erasePreview(
        x: Float,
        y: Float,
        radius: Float,
        page: String,
    ): String =
        scoped(
            JSONObject()
                .put("t", "live")
                .put("kind", "erase")
                .put("x", x.toDouble())
                .put("y", y.toDouble())
                .put("r", radius.toDouble()),
            page,
        ).toString()

    /** Asks the server to make `page` the active one (it replies with that page's snapshot). */
    fun select(page: String): String = JSONObject().put("t", "select").put("page", page).toString()

    /** Adds the page every page-scoped frame carries; blank pages are left alone for back-compat. */
    private fun scoped(
        frame: JSONObject,
        page: String,
    ): JSONObject = if (page.isEmpty()) frame else frame.put("page", page)

    fun liveBegin(
        id: Int,
        preset: Int,
        argb: Int,
        scale: Float,
        seed: Long,
        page: String,
    ): String =
        scoped(
            JSONObject()
                .put("t", "live")
                .put("id", id)
                .put("first", true)
                .put("preset", preset)
                .put("argb", argb)
                .put("scale", scale.toDouble())
                .put("seed", seed.toString())
                .put("samples", JSONArray())
                .put("times", JSONArray()),
            page,
        ).toString()

    fun liveSamples(
        id: Int,
        samples: FloatArray,
        times: IntArray,
        count: Int,
        page: String,
    ): String {
        val floats = JSONArray()
        for (i in 0 until count * SAMPLE_STRIDE) floats.put(samples[i].toDouble())
        return scoped(
            JSONObject()
                .put("t", "live")
                .put("id", id)
                .put("samples", floats)
                .put("times", times(times, count)),
            page,
        ).toString()
    }

    /** Parses a `{"t":"page","strokes":[…]}` frame; `null` for anything else or malformed JSON. */
    fun parsePage(text: String): List<SavedStroke>? {
        val frame = runCatching { JSONObject(text) }.getOrNull() ?: return null
        if (frame.optString("t") != "page") return null
        val array = frame.optJSONArray("strokes") ?: return emptyList()
        return (0 until array.length()).mapNotNull { runCatching { parseStroke(array.getJSONObject(it)) }.getOrNull() }
    }

    /** Parses one stroke object; throws on a malformed shape so the caller can skip it. */
    fun parseStroke(json: JSONObject): SavedStroke {
        val samples = json.getJSONArray("samples")
        val times = json.getJSONArray("times")
        val count = times.length()
        require(samples.length() == count * SAMPLE_STRIDE) { "sample count mismatch" }
        val floats = FloatArray(samples.length()) { samples.getDouble(it).toFloat() }
        val micros = IntArray(count) { times.getInt(it) }
        return SavedStroke(
            presetIndex = json.getInt("preset"),
            argb = json.getInt("argb"),
            sizeScale = json.optDouble("scale", 1.0).toFloat(),
            seed = json.getString("seed").toLong(),
            samples = floats,
            times = micros,
            count = count,
        )
    }

    fun liveCancel(
        id: Int,
        page: String,
    ): String =
        scoped(
            JSONObject()
                .put("t", "live")
                .put("id", id)
                .put("cancel", true),
            page,
        ).toString()

    private fun times(
        times: IntArray,
        count: Int,
    ): JSONArray {
        val array = JSONArray()
        for (i in 0 until count) array.put(times[i])
        return array
    }
}
