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

    fun page(strokes: List<SavedStroke>): String {
        val array = JSONArray()
        strokes.forEach { array.put(stroke(it)) }
        return JSONObject().put("t", "page").put("strokes", array).toString()
    }

    fun strokeFrame(stroke: SavedStroke): String =
        JSONObject().put("t", "stroke").put("stroke", stroke(stroke)).toString()

    fun liveBegin(
        id: Int,
        preset: Int,
        argb: Int,
        scale: Float,
        seed: Long,
    ): String =
        JSONObject()
            .put("t", "live")
            .put("id", id)
            .put("first", true)
            .put("preset", preset)
            .put("argb", argb)
            .put("scale", scale.toDouble())
            .put("seed", seed.toString())
            .put("samples", JSONArray())
            .put("times", JSONArray())
            .toString()

    fun liveSamples(
        id: Int,
        samples: FloatArray,
        times: IntArray,
        count: Int,
    ): String {
        val floats = JSONArray()
        for (i in 0 until count * SAMPLE_STRIDE) floats.put(samples[i].toDouble())
        return JSONObject()
            .put("t", "live")
            .put("id", id)
            .put("samples", floats)
            .put("times", times(times, count))
            .toString()
    }

    fun liveCancel(id: Int): String =
        JSONObject()
            .put("t", "live")
            .put("id", id)
            .put("cancel", true)
            .toString()

    private fun times(
        times: IntArray,
        count: Int,
    ): JSONArray {
        val array = JSONArray()
        for (i in 0 until count) array.put(times[i])
        return array
    }
}
