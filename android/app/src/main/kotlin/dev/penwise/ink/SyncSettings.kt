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

    /** The page that was on screen last (32 hex characters), or `null` on first run. */
    var activePage: String?
        get() = prefs.getString(KEY_ACTIVE, null)?.takeIf { it.isNotBlank() }
        set(value) {
            prefs.edit { putString(KEY_ACTIVE, value) }
        }

    /**
     * Endpoints in try order, each as the `/sync` WebSocket URL (the token travels in the `Hello` frame): the
     * override first, then the built-in candidates, so a stale override (an `adb reverse` address after the cable is
     * gone) falls through to the LAN and Tailscale.
     */
    fun endpoints(): List<String> = endpointList(overrideUrl, candidates())

    private fun candidates(): List<String> = context.resources.getStringArray(R.array.sync_urls).toList()

    private companion object {
        const val PREFS = "penwise"
        const val KEY_URL = "sync_url"
        const val KEY_TOKEN = "sync_token"
        const val KEY_ACTIVE = "active_page"
    }
}

/** The override (when set) followed by the built-in [candidates], as socket URLs without duplicates. */
internal fun endpointList(
    override: String?,
    candidates: List<String>,
): List<String> =
    (listOfNotNull(override) + candidates)
        .map(::toWebSocketUrl)
        .filter { it.isNotEmpty() }
        .distinct()

/** [endpoints] with the one currently connected (if it is among them) moved to the front, for HTTP fetches. */
internal fun connectedFirst(
    connected: String?,
    endpoints: List<String>,
): List<String> =
    if (connected != null && connected in endpoints) listOf(connected) + (endpoints - connected) else endpoints

/** Path of the binary sync socket on the server. */
internal const val SOCKET_PATH = "/sync"

private const val HTTP = "http://"
private const val SECURE_HTTP = "https://"
private const val SOCKET = "ws://"
private const val SECURE_SOCKET = "wss://"

/**
 * Turns whatever was typed or scanned into the WebSocket URL sync dials.
 *
 * Pairing hands over the viewer's `http://host:7420`, and people paste that too; the socket lives at
 * `ws://host:7420/sync`; whatever path was typed is replaced.
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

/** What saving the sync dialog (or a pairing link) changes: [url] `null` = keep the saved address. */
internal class SyncUpdate(
    val url: String?,
    val token: String,
)

/**
 * The settings to save for [url] and [token]. A pairing code is exchanged through [pair] (`(token, server url)`)
 * first; when that fails this returns `null` and nothing may be saved: the code is not a token, and overwriting the
 * working address and token with it locks the tablet out.
 */
internal fun syncUpdateFor(
    url: String?,
    token: String,
    pair: (url: String?, code: String) -> Pair<String, String>?,
): SyncUpdate? {
    val trimmed = token.trim()
    if (!LibraryClient.looksLikeCode(trimmed)) return SyncUpdate(url, token)
    val paired = pair(url, trimmed.replace("-", "").uppercase()) ?: return null
    return SyncUpdate(paired.second.ifBlank { url.orEmpty() }.ifBlank { null }, paired.first)
}
