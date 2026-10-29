package dev.penwise.frames

import java.net.URLEncoder

/**
 * Where a frame's page lives: the project's own port on the host the tablet syncs with (§6.1, C2).
 *
 * From another machine the project ports are protected by the sync token, and a browser cannot send a header for a
 * page load, so the first load goes through `/__penwise/enter`, which sets the cookie and redirects to the route. On
 * loopback the server lets same-machine requests through, and the route is loaded directly.
 */
internal object FrameUrls {
    /** `http://<host>:<port><route>` (loopback) or `.../__penwise/enter?token=…&to=<route>` (anything else). */
    fun frameUrl(
        httpBase: String?,
        port: Int,
        route: String,
        token: String,
    ): String? {
        val host = hostOf(httpBase) ?: return null
        val path = FrameRoutes.normalize(route) ?: return null
        val authority = authority(host, port)
        return if (isLoopback(host)) {
            "http://$authority$path"
        } else {
            "http://$authority/$ENTER_PATH?token=${encode(token)}&to=${encode(path)}"
        }
    }

    /** `scheme://authority` of [url], the part that says which server (and project port) a page came from. */
    fun origin(url: String?): String? {
        val value = url ?: return null
        val split = value.indexOf(SCHEME)
        if (split < 0) return null
        val prefix = value.substring(0, split + SCHEME.length)
        val authority = value.substring(split + SCHEME.length).substringBefore('/')
        if (authority.isEmpty()) return null
        return prefix + authority
    }

    /** The host of an `http://host:port` base, without the port; `null` when it carries no host. */
    fun hostOf(httpBase: String?): String? {
        val value = httpBase ?: return null
        val split = value.indexOf(SCHEME)
        if (split < 0) return null
        val authority = value.substring(split + SCHEME.length).substringBefore('/')
        if (authority.isEmpty()) return null
        // An IPv6 literal is bracketed: `http://[::1]:17421`.
        return if (authority.startsWith("[")) authority.substringBefore(']') + "]" else authority.substringBefore(':')
    }

    /** The tablet's own machine: the server trusts it without the token. */
    fun isLoopback(host: String): Boolean =
        host == "localhost" ||
            host == "[::1]" ||
            host == "::1" ||
            host.startsWith("127.")

    private fun authority(
        host: String,
        port: Int,
    ): String = if (host.contains(':') && !host.startsWith("[")) "[$host]:$port" else "$host:$port"

    private fun encode(value: String): String = URLEncoder.encode(value, Charsets.UTF_8.name())

    private const val SCHEME = "://"
    private const val ENTER_PATH = "__penwise/enter"
}
