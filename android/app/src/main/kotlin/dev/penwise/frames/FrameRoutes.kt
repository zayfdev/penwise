package dev.penwise.frames

/**
 * The route of a frame's page: a path plus optional query, always starting at `/` (the contract's `route` field).
 * Accepts what people type: a bare path (`about`), a full URL (`http://host:17421/work?x=1`), or nothing (`/`).
 */
object FrameRoutes {
    /** `path` as a route, or `null` when it cannot be one (empty after trimming, or too long for the item field). */
    fun normalize(path: String): String? {
        val trimmed = path.trim()
        val route =
            when {
                trimmed.isEmpty() -> ROOT
                trimmed.contains("://") -> "/" + trimmed.substringAfter("://").substringAfter('/', "")
                trimmed.startsWith('/') -> trimmed
                else -> "/$trimmed"
            }
        if (route.length > MAX_LENGTH) return null
        return route
    }

    /** The route a loaded document's [url] shows: its path and query, without the host. */
    fun ofUrl(url: String): String? {
        val afterScheme = url.substringAfter("://", missingDelimiterValue = "")
        if (afterScheme.isEmpty()) return null
        val rest = afterScheme.substringAfter('/', missingDelimiterValue = "")
        return normalize(if (rest.isEmpty()) ROOT else "/$rest")
    }

    const val ROOT = "/"

    /** The item field's cap (C1). */
    private const val MAX_LENGTH = 2048
}
