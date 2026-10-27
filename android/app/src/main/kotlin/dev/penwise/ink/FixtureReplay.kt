package dev.penwise.ink

import android.content.Context
import android.util.Log
import org.json.JSONObject
import java.io.File

/** One recorded stroke: interleaved samples (x, y, pressure, tilt, azimuth) and their times in µs. */
class ReplayStroke(
    val preset: Int,
    val argb: Int,
    val scale: Float,
    val seed: Long,
    val samples: FloatArray,
    val times: IntArray,
)

/**
 * Debug-only replay: reads a recorded page (`penwise-fixtures strokes-json`) and hands its strokes on.
 *
 * This exists so the whole tablet pipeline — scene, sync, server — can be exercised with real multi-sample strokes
 * without a hand on the screen. It is wired up only in debug builds.
 */
object FixtureReplay {
    private const val TAG = "PenwiseFixture"
    private const val STRIDE = 5

    /** Replays [path] through [scene]: every stroke goes the scene → commit → sync path a pen stroke takes. */
    fun replayOnto(
        context: Context,
        path: String,
        scene: PageScene,
    ): Int =
        replay(context, path) { stroke ->
            val edits =
                scene.addStroke(
                    scene.newItemId(),
                    stroke.preset,
                    stroke.argb,
                    stroke.scale,
                    stroke.seed,
                    stroke.samples,
                    stroke.times,
                )
            scene.commit(edits)
        }

    /** Parses a page file and hands every stroke to [onStroke]; returns how many were found. */
    fun replay(
        context: Context,
        path: String,
        onStroke: (ReplayStroke) -> Unit,
    ): Int {
        val file = File(path).takeIf { it.isAbsolute } ?: File(context.filesDir, path)
        if (!file.isFile) {
            Log.w(TAG, "no fixture at ${file.absolutePath}")
            return 0
        }
        val strokes = runCatching { JSONObject(file.readText()) }.getOrNull()?.optJSONArray("strokes")
        if (strokes == null) {
            Log.w(TAG, "fixture has no strokes array")
            return 0
        }
        var added = 0
        for (index in 0 until strokes.length()) {
            val stroke = runCatching { parse(strokes.getJSONObject(index)) }.getOrNull() ?: continue
            onStroke(stroke)
            added++
        }
        Log.i(TAG, "replayed $added strokes from ${file.name}")
        return added
    }

    /** One stroke object: `preset`, `argb`, `scale`, `seed` (string), `samples` (5 per sample), `times`. */
    internal fun parse(json: JSONObject): ReplayStroke {
        val samples = json.getJSONArray("samples")
        val times = json.getJSONArray("times")
        require(samples.length() == times.length() * STRIDE) { "sample count mismatch" }
        return ReplayStroke(
            preset = json.getInt("preset"),
            argb = json.getInt("argb"),
            scale = json.optDouble("scale", 1.0).toFloat(),
            seed = json.getString("seed").toLong(),
            samples = FloatArray(samples.length()) { samples.getDouble(it).toFloat() },
            times = IntArray(times.length()) { times.getInt(it) },
        )
    }
}
