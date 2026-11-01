package dev.penwise.overview

import dev.penwise.sync.Ids
import org.json.JSONArray
import org.json.JSONException
import org.json.JSONObject

/** A sketch page attached to a strip (contract §2.3): the [page] (32-hex, as the tablet names it) and its screen. */
internal data class StripPage(
    val page: String,
    val route: String,
    val preset: String,
)

/** `GET /projects/{p}/strip-pages` (contract §2.3), parsed. */
internal object StripPagesJson {
    /**
     * The attached pages in attach order, or null when [json] is not a list. A bare array is the contract's shape; an
     * object with a `pages` array is read the same way. The server names pages by ULID: they become the tablet's
     * 32-hex ids here, and a row whose id is not a ULID is skipped.
     */
    fun parse(json: String): List<StripPage>? =
        try {
            val trimmed = json.trim()
            val array = if (trimmed.startsWith("[")) JSONArray(trimmed) else JSONObject(trimmed).optJSONArray("pages")
            array?.let { list ->
                (0 until list.length()).mapNotNull { index ->
                    val entry = list.optJSONObject(index)
                    val page = Ids.fromUlid(entry?.optString("page").orEmpty())
                    val route = entry?.optString("route").orEmpty()
                    if (page == null || route.isEmpty()) {
                        null
                    } else {
                        StripPage(page, route, entry?.optString("preset").orEmpty())
                    }
                }
            }
        } catch (ignored: JSONException) {
            null
        }
}
