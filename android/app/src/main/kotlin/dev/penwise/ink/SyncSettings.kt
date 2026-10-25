package dev.penwise.ink

import android.content.Context
import androidx.core.content.edit
import dev.penwise.R

/**
 * Where and how to reach the Penwise PC server.
 *
 * The URL override and token live in shared preferences, so they survive restarts and can be changed from the
 * toolbar without rebuilding. Empty override = the built-in candidates (home Wi-Fi, then Tailscale).
 */
class SyncSettings(
    private val context: Context,
) {
    private val prefs = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)

    var overrideUrl: String?
        get() = prefs.getString(KEY_URL, null)?.takeIf { it.isNotBlank() }
        set(value) {
            prefs.edit { putString(KEY_URL, value) }
        }

    var token: String
        get() = prefs.getString(KEY_TOKEN, "") ?: ""
        set(value) {
            prefs.edit { putString(KEY_TOKEN, value) }
        }

    /** The URL to show in the settings dialog: the override, or the first built-in candidate. */
    fun displayedUrl(): String = overrideUrl ?: candidates().firstOrNull().orEmpty()

    /** Candidates in try order, each as a WebSocket URL with the token appended when one is set. */
    fun endpoints(): List<String> {
        val bases = overrideUrl?.let { listOf(it) } ?: candidates()
        return bases.map { withToken(toWebSocketUrl(it)) }
    }

    private fun candidates(): List<String> = context.resources.getStringArray(R.array.sync_urls).toList()

    private fun withToken(base: String): String {
        if (token.isEmpty()) return base
        val separator = if ('?' in base) '&' else '?'
        return "$base${separator}token=$token"
    }

    private companion object {
        const val PREFS = "penwise"
        const val KEY_URL = "sync_url"
        const val KEY_TOKEN = "sync_token"
    }
}

/** Path of the sync socket on the server. */
internal const val SOCKET_PATH = "/ws"

private const val HTTP = "http://"
private const val SECURE_HTTP = "https://"
private const val SOCKET = "ws://"
private const val SECURE_SOCKET = "wss://"

/**
 * Turns whatever was typed or scanned into the WebSocket URL sync dials.
 *
 * Pairing hands over the viewer's `http://host:7420`, and people paste that too; the socket lives at
 * `ws://host:7420/ws`. Anything already pointing at `/ws` comes back unchanged.
 */
internal fun toWebSocketUrl(url: String): String {
    val trimmed = url.trim()
    if (trimmed.isEmpty()) return trimmed
    val (scheme, rest) =
        when {
            trimmed.startsWith(SECURE_SOCKET, ignoreCase = true) -> {
                SECURE_SOCKET to trimmed.substring(SECURE_SOCKET.length)
            }

            trimmed.startsWith(SOCKET, ignoreCase = true) -> {
                SOCKET to trimmed.substring(SOCKET.length)
            }

            trimmed.startsWith(SECURE_HTTP, ignoreCase = true) -> {
                SECURE_SOCKET to trimmed.substring(SECURE_HTTP.length)
            }

            trimmed.startsWith(HTTP, ignoreCase = true) -> {
                SOCKET to trimmed.substring(HTTP.length)
            }

            else -> {
                SOCKET to trimmed
            }
        }
    val authority = rest.substringBefore('/').substringBefore('?')
    return scheme + authority + SOCKET_PATH
}
