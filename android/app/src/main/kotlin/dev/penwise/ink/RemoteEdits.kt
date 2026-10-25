package dev.penwise.ink

import android.util.Log
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.json.JSONArray
import org.json.JSONObject

/**
 * Frames other clients produce while this tablet shows the same page: a stroke drawn in the browser, an agent's
 * mark, or a removal. They are applied as [RemoteOp]s, so they never enter this device's undo history.
 */
internal object RemoteEdits {
    /** Merges one `stroke` frame; `merge` is the store's seed-deduplicating path. */
    fun applyStroke(
        store: StrokeStore,
        scope: kotlinx.coroutines.CoroutineScope,
        frame: JSONObject,
    ) {
        val wire = frame.optJSONObject("stroke") ?: return
        scope.launch {
            val stroke =
                withContext(Dispatchers.Default) {
                    Wire.parseStroke(wire).also { it.rebuild() }
                }
            if (store.merge(listOf(stroke))) {
                Log.i("PenwiseSync", "merged a stroke drawn elsewhere")
                store.persist()
            }
        }
    }

    /** Applies one `op` frame, without echoing it back. */
    fun applyOp(
        store: StrokeStore,
        scope: kotlinx.coroutines.CoroutineScope,
        op: JSONObject?,
    ) {
        if (op == null) return
        scope.launch {
            val remote = withContext(Dispatchers.Default) { read(op) } ?: return@launch
            store.applyRemote(remote)
        }
    }

    /** Maps an op frame onto a [RemoteOp]; `null` for kinds this device does not apply. */
    private fun read(op: JSONObject): RemoteOp? =
        when (op.optString("k")) {
            "remove" -> RemoteOp.Remove(seeds(op.optJSONArray("seeds")))
            "clear" -> RemoteOp.Clear
            "replace" -> RemoteOp.Replace(seeds(op.optJSONArray("seeds")), strokes(op.optJSONArray("strokes")))
            "add" -> RemoteOp.Add(listOfNotNull(op.optJSONObject("stroke")?.let(::rebuiltStroke)))
            else -> null
        }

    private fun strokes(array: JSONArray?): List<SavedStroke> {
        if (array == null) return emptyList()
        return (0 until array.length()).mapNotNull { index -> array.optJSONObject(index)?.let(::rebuiltStroke) }
    }

    private fun rebuiltStroke(json: JSONObject): SavedStroke = Wire.parseStroke(json).also { it.rebuild() }

    private fun seeds(array: JSONArray?): Set<Long> {
        if (array == null) return emptySet()
        return (0 until array.length()).mapNotNull { index -> array.optString(index).toLongOrNull() }.toSet()
    }
}
