package dev.penwise.ink

import android.util.Log
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.json.JSONArray
import org.json.JSONObject

/**
 * Frames other clients produce while this tablet shows the same page: a stroke drawn in the browser, an agent's
 * mark, or a removal. They are applied as [RemoteOp]s, so they never enter this device's undo history.
 *
 * Peers are not trusted to send well-formed strokes: a frame that does not parse, or whose stroke the engine
 * refuses to build, is logged and skipped. It must never take the tablet down.
 */
internal object RemoteEdits {
    private const val TAG = "PenwiseSync"

    /** Where a skipped frame is reported; swapped in tests, where `android.util.Log` is unavailable. */
    internal var warn: (String) -> Unit = { Log.w(TAG, it) }

    /**
     * Merges one `stroke` frame; `merge` is the store's seed-deduplicating path. `isCurrent` says whether the page
     * the frame was for is still the open one once its geometry is ready.
     */
    fun applyStroke(
        store: StrokeStore,
        scope: CoroutineScope,
        frame: JSONObject,
        isCurrent: () -> Boolean,
    ) {
        val wire = frame.optJSONObject("stroke") ?: return
        scope.launch {
            val stroke = withContext(Dispatchers.Default) { rebuiltStroke(wire) } ?: return@launch
            // The page may have been switched while the geometry was built; the stroke belongs to the old one.
            if (!isCurrent()) return@launch
            if (store.merge(listOf(stroke))) {
                Log.i(TAG, "merged a stroke drawn elsewhere")
                store.persist()
            }
        }
    }

    /** Applies one `op` frame, without echoing it back. */
    fun applyOp(
        store: StrokeStore,
        scope: CoroutineScope,
        op: JSONObject?,
        isCurrent: () -> Boolean,
    ) {
        if (op == null) return
        scope.launch {
            val remote = withContext(Dispatchers.Default) { read(op) } ?: return@launch
            if (!isCurrent()) return@launch
            store.applyRemote(remote)
        }
    }

    /** Maps an op frame onto a [RemoteOp]; `null` for kinds this device does not apply or a malformed frame. */
    internal fun read(
        op: JSONObject,
        build: (SavedStroke) -> Unit = { it.rebuild() },
    ): RemoteOp? =
        when (op.optString("k")) {
            "remove" -> {
                RemoteOp.Remove(seeds(op.optJSONArray("seeds")))
            }

            "clear" -> {
                RemoteOp.Clear
            }

            "replace" -> {
                RemoteOp.Replace(seeds(op.optJSONArray("seeds")), strokes(op.optJSONArray("strokes"), build))
            }

            "add" -> {
                RemoteOp.Add(listOfNotNull(op.optJSONObject("stroke")?.let { rebuiltStroke(it, build) }))
            }

            else -> {
                null
            }
        }

    private fun strokes(
        array: JSONArray?,
        build: (SavedStroke) -> Unit,
    ): List<SavedStroke> {
        if (array == null) return emptyList()
        return (0 until array.length()).mapNotNull { index ->
            array.optJSONObject(index)?.let { rebuiltStroke(it, build) }
        }
    }

    /** The stroke in `json` with geometry, or `null` (after a warning) when it is malformed or unbuildable. */
    internal fun rebuiltStroke(
        json: JSONObject,
        build: (SavedStroke) -> Unit = { it.rebuild() },
    ): SavedStroke? {
        val stroke =
            try {
                Wire.parseStroke(json)
            } catch (error: org.json.JSONException) {
                warn("skipping malformed stroke: ${error.message}")
                return null
            } catch (error: IllegalArgumentException) {
                warn("skipping malformed stroke: ${error.message}")
                return null
            }
        val built =
            StrokeRebuild.tryBuild(stroke, build) { skipped, error ->
                warn("skipping stroke ${skipped.seed}: ${error.message}")
            }
        return stroke.takeIf { built }
    }

    private fun seeds(array: JSONArray?): Set<Long> {
        if (array == null) return emptySet()
        return (0 until array.length()).mapNotNull { index -> array.optString(index).toLongOrNull() }.toSet()
    }
}
