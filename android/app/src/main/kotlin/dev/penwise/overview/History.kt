package dev.penwise.overview

import org.json.JSONArray
import org.json.JSONException
import org.json.JSONObject

/** One kept picture of a generation: its Image [item] on the board, the [asset] with its bytes, and its size. */
internal data class HistoryPicture(
    val item: String,
    val asset: String,
    val width: Int,
    val height: Int,
)

/** A variant picture of a generation; [picked] is the one the owner chose when the round resolved. */
internal data class HistoryVariant(
    val label: String,
    val picture: HistoryPicture,
    val picked: Boolean,
)

/** One resolved round of a lane (contract §2.2): the OG picture and one picture per variant. */
internal data class Generation(
    val round: Int,
    val resolvedMs: Long,
    val og: HistoryPicture,
    val variants: List<HistoryVariant>,
) {
    /** The picked variant, or null when none was (or no variants exist). */
    val picked: HistoryVariant? get() = variants.firstOrNull { it.picked }
}

/** `GET /projects/{p}/history` (contract §2.2), parsed. Pure, so the shape is unit-tested. */
internal object HistoryJson {
    /**
     * The generations in [json], in the order the server sent them (newest first); null for anything that is not a
     * history reply. A generation without an OG picture or with an unreadable one is left out, as is a variant
     * without an asset.
     */
    fun parse(json: String): List<Generation>? =
        try {
            val array = JSONObject(json).optJSONArray("generations") ?: return null
            (0 until array.length()).mapNotNull { generation(array.optJSONObject(it)) }
        } catch (ignored: JSONException) {
            null
        }

    private fun generation(entry: JSONObject?): Generation? {
        val og = picture(entry?.optJSONObject("og")) ?: return null
        return Generation(
            round = entry?.optInt("round") ?: return null,
            resolvedMs = entry.optLong("resolved_ms"),
            og = og,
            variants = variants(entry.optJSONArray("variants")),
        )
    }

    private fun variants(array: JSONArray?): List<HistoryVariant> =
        (0 until (array?.length() ?: 0)).mapNotNull { index ->
            val entry = array?.optJSONObject(index)
            picture(entry)?.let {
                HistoryVariant(
                    entry?.optString("label").orEmpty(),
                    it,
                    entry?.optBoolean("picked") == true,
                )
            }
        }

    private fun picture(entry: JSONObject?): HistoryPicture? {
        val asset = entry?.optString("asset").orEmpty()
        val item = entry?.optString("item").orEmpty()
        if (asset.isEmpty() || item.isEmpty()) return null
        return HistoryPicture(item, asset, entry?.optInt("w") ?: 0, entry?.optInt("h") ?: 0)
    }
}
