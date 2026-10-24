package dev.penwise.ink

import android.util.Log
import java.io.IOException
import java.io.OutputStreamWriter
import java.net.HttpURLConnection
import java.net.URL

/**
 * Library mutations over plain HTTP (`/pages`, `/notebooks`), because the WebSocket is for page content.
 *
 * Every call is best-effort: the tablet is offline-first, so a failure only means the server learns about the
 * page later. Calls block, so they run off the main thread.
 */
class LibraryClient(
    private val baseUrl: String,
    private val token: String,
) {
    /** `POST /pages`; returns the response body, or `null` on any failure. */
    fun createPage(body: String): String? = send("POST", "/pages", body)

    /** `PATCH /pages/<id>`. */
    fun updatePage(
        id: String,
        body: String,
    ): String? = send("PATCH", "/pages/$id", body)

    /** `DELETE /pages/<id>`. */
    fun deletePage(id: String): String? = send("DELETE", "/pages/$id", null)

    /** The library index, or `null` when the server cannot be reached. */
    fun pages(): String? = send("GET", "/pages", null)

    /** One library call; `null` on any failure. */
    fun request(
        method: String,
        path: String,
        body: String?,
    ): String? = send(method, path, body)

    private fun send(
        method: String,
        path: String,
        body: String?,
    ): String? {
        val url = URL(base(httpBase(baseUrl), path))
        val connection =
            runCatching { url.openConnection() as HttpURLConnection }
                .getOrNull()
                ?: return null
        return try {
            connection.requestMethod = method
            connection.connectTimeout = TIMEOUT_MS
            connection.readTimeout = TIMEOUT_MS
            connection.setRequestProperty("Accept", "application/json")
            if (token.isNotEmpty()) connection.setRequestProperty("X-Penwise-Token", token)
            if (body != null) {
                connection.doOutput = true
                connection.setRequestProperty("Content-Type", "application/json")
                OutputStreamWriter(connection.outputStream, Charsets.UTF_8).use { it.write(body) }
            }
            val code = connection.responseCode
            val ok = code in HTTP_OK_FIRST..HTTP_OK_LAST
            val stream = if (ok) connection.inputStream else connection.errorStream
            val text = stream?.bufferedReader()?.use { it.readText() }.orEmpty()
            if (ok) text else null.also { Log.w(TAG, "$method $path -> $code $text") }
        } catch (error: IOException) {
            Log.w(TAG, "$method $path failed: ${error.message}")
            null
        } finally {
            connection.disconnect()
        }
    }

    /** `ws://host:port/ws?token=…` becomes the `http://host:port` base; extra query parts are dropped. */
    private fun httpBase(wsUrl: String): String =
        wsUrl
            .substringBefore('?')
            .removeSuffix("/ws")
            .replaceFirst("wss://", "https://")
            .replaceFirst("ws://", "http://")

    private fun base(
        root: String,
        path: String,
    ): String = root.trimEnd('/') + path + (if (token.isEmpty()) "" else "?token=$token")

    private companion object {
        const val TAG = "PenwiseLibrary"
        const val TIMEOUT_MS = 4_000
        const val HTTP_OK_FIRST = 200
        const val HTTP_OK_LAST = 299
    }
}
