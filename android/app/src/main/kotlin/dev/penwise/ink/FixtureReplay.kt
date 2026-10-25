package dev.penwise.ink

import android.content.Context
import android.util.Log
import org.json.JSONObject
import java.io.File

/**
 * Debug-only replay: drops a recorded page (`penwise-fixtures strokes-json`) into the store.
 *
 * This exists so the whole tablet pipeline — store, file, sync, reload — can be exercised with real multi-sample
 * strokes without a hand on the screen. It is wired up only in debug builds.
 */
object FixtureReplay {
    private const val TAG = "PenwiseFixture"

    /** Parses a page frame file and hands every stroke to [onStroke]; returns how many were added. */
    fun replay(
        context: Context,
        path: String,
        onStroke: (SavedStroke) -> Unit,
    ): Int {
        val file = File(path).takeIf { it.isAbsolute } ?: File(context.filesDir, path)
        if (!file.isFile) {
            Log.w(TAG, "no fixture at ${file.absolutePath}")
            return 0
        }
        val frame = runCatching { JSONObject(file.readText()) }.getOrNull()
        val strokes = frame?.optJSONArray("strokes")
        if (strokes == null) {
            Log.w(TAG, "fixture has no strokes array")
            return 0
        }
        var added = 0
        for (index in 0 until strokes.length()) {
            val stroke =
                runCatching { Wire.parseStroke(strokes.getJSONObject(index)) }.getOrNull()
                    ?: continue
            stroke.rebuild()
            onStroke(stroke)
            added++
        }
        Log.i(TAG, "replayed $added strokes from ${file.name}")
        return added
    }
}
