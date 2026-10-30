package dev.penwise.frames

import org.json.JSONArray
import org.json.JSONException
import org.json.JSONObject
import kotlin.math.max

/** One PNG strip of a still: [height] CSS px of the document starting at document y [y]. */
data class StillStrip(
    val y: Int,
    val height: Int,
    val url: String,
)

/**
 * `GET /frames/{id}/still` (contract §4), parsed: the document and viewport sizes in CSS px, the strips that tile the
 * document, and the layout the core takes while the frame is not live (rects in document coordinates).
 */
data class StillManifest(
    val version: String,
    val docWidth: Int,
    val docHeight: Int,
    val viewportWidth: Int,
    val viewportHeight: Int,
    val strips: List<StillStrip>,
    val rects: Map<String, FrameRect?>,
    val state: String,
) {
    /**
     * The layout of a frame that is not live, as the bridge would report it scrolled to the top: the still's rects
     * are document coordinates, which at scroll 0 are the viewport coordinates the core's merge expects.
     */
    fun asLayout(): FrameMessage.Layout =
        FrameMessage.Layout(
            scroll = listOf(0f, 0f),
            doc = listOf(docWidth.toFloat(), docHeight.toFloat()),
            viewport = listOf(viewportWidth.toFloat(), viewportHeight.toFloat()),
            state = state,
            rects = rects,
        )
}

/** The still JSON and the choice of strips to show. Pure, so both are unit-tested. */
object StillManifests {
    /** The manifest in [json]; null for anything the server would not send. */
    fun parse(json: String): StillManifest? =
        try {
            val root = JSONObject(json)
            val doc = root.optJSONArray("doc")
            val viewport = root.optJSONArray("viewport")
            val layout = root.optJSONObject("layout")
            StillManifest(
                version = root.opt("version")?.toString().orEmpty(),
                docWidth = doc?.optInt(0) ?: 0,
                docHeight = doc?.optInt(1) ?: 0,
                viewportWidth = viewport?.optInt(0) ?: 0,
                viewportHeight = viewport?.optInt(1) ?: 0,
                strips = strips(root.optJSONArray("strips")),
                rects = rects(layout?.optJSONObject("rects")),
                state = layout?.optString("state").orEmpty(),
            ).takeIf { it.version.isNotEmpty() }
        } catch (ignored: JSONException) {
            null
        }

    private fun rects(objects: JSONObject?): Map<String, FrameRect?> {
        if (objects == null) return emptyMap()
        val out = LinkedHashMap<String, FrameRect?>()
        for (key in objects.keys()) {
            val rect = objects.optJSONArray(key)
            out[key] =
                if (rect == null || rect.length() < RECT_VALUES) {
                    null
                } else {
                    val values = List(RECT_VALUES) { rect.optDouble(it).toFloat() }
                    FrameRect(values[X], values[Y], values[WIDTH], values[HEIGHT])
                }
        }
        return out
    }

    private const val RECT_VALUES = 4
    private const val X = 0
    private const val Y = 1
    private const val WIDTH = 2
    private const val HEIGHT = 3

    private fun strips(array: JSONArray?): List<StillStrip> {
        if (array == null) return emptyList()
        return (0 until array.length()).mapNotNull { index ->
            val strip = array.optJSONObject(index) ?: return@mapNotNull null
            val url = strip.optString("url")
            val height = strip.optInt("h")
            if (url.isEmpty() || height <= 0) null else StillStrip(strip.optInt("y"), height, url)
        }
    }
}

/** Which strips a view needs, and how far to shrink them. */
object StillStrips {
    /**
     * The strips that overlap the document rows [docTop]..[docBottom] (CSS px), with [margin] extra on both sides so
     * a small pan does not flash an empty band.
     */
    fun visible(
        strips: List<StillStrip>,
        docTop: Float,
        docBottom: Float,
        margin: Float = 0f,
    ): List<StillStrip> = strips.filter { overlaps(it, docTop, docBottom, margin) }

    /** Does [strip] overlap the document rows [docTop]..[docBottom] (CSS px) widened by [margin]? */
    fun overlaps(
        strip: StillStrip,
        docTop: Float,
        docBottom: Float,
        margin: Float = 0f,
    ): Boolean = strip.y < docBottom + margin && strip.y + strip.height > docTop - margin

    /**
     * The power-of-two `inSampleSize` that keeps a strip [imageWidth] px wide at least [targetWidth] px wide: a
     * long page seen from far away is decoded at screen resolution, not at its full size.
     */
    fun sampleSize(
        imageWidth: Int,
        targetWidth: Int,
    ): Int {
        var sample = 1
        val target = max(targetWidth, 1)
        while (imageWidth / (sample * 2) >= target) sample *= 2
        return sample
    }
}
