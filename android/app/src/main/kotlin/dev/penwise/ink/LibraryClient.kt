package dev.penwise.ink

import android.util.Log
import org.json.JSONObject
import java.io.IOException
import java.io.OutputStreamWriter
import java.net.HttpURLConnection
import java.net.URL

/**
 * The two things the tablet still asks the server over plain HTTP: pairing (`POST /pair`) and picture assets
 * (`GET /assets/<hash>`). Everything else travels over `/sync`.
 *
 * Every call is best-effort and blocks, so callers run it off the main thread.
 */
class LibraryClient(
    private val baseUrl: String,
    private val token: String,
) {
    /** Exchanges a pairing code for the server's token; `null` when the code is wrong or the server is away. */
    fun pair(code: String): Pair<String, String>? {
        val body = JSONObject().put("code", code).toString()
        val reply = send("POST", "/pair", body) ?: return null
        val json = runCatching { JSONObject(reply) }.getOrNull() ?: return null
        val token = json.optString("token").takeIf { it.isNotEmpty() } ?: return null
        val urls = json.optJSONArray("urls")
        val url = if (urls != null && urls.length() > 0) urls.optString(0) else ""
        return token to url
    }

    /** `GET /assets/<id>` decoded as a bitmap; `null` when the server is away or the asset is gone. */
    fun asset(asset: String): android.graphics.Bitmap? {
        val url = URL(base(httpBase(baseUrl), "/assets/$asset"))
        val connection =
            runCatching { url.openConnection() as HttpURLConnection }.getOrNull() ?: return null
        return try {
            connection.requestMethod = "GET"
            connection.connectTimeout = TIMEOUT_MS
            connection.readTimeout = TIMEOUT_MS
            if (token.isNotEmpty()) connection.setRequestProperty("X-Penwise-Token", token)
            if (connection.responseCode !in HTTP_OK_FIRST..HTTP_OK_LAST) {
                Log.w(TAG, "GET /assets/$asset -> ${connection.responseCode}")
                null
            } else {
                android.graphics.BitmapFactory.decodeStream(connection.inputStream)
            }
        } catch (error: IOException) {
            Log.w(TAG, "GET /assets/$asset failed: ${error.message}")
            null
        } finally {
            connection.disconnect()
        }
    }

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

    /** `ws://host:port/sync` becomes the `http://host:port` base; extra query parts are dropped. */
    private fun httpBase(wsUrl: String): String =
        wsUrl
            .substringBefore('?')
            .removeSuffix("/sync")
            .replaceFirst("wss://", "https://")
            .replaceFirst("ws://", "http://")

    private fun base(
        root: String,
        path: String,
    ): String = root.trimEnd('/') + path + (if (token.isEmpty()) "" else "?token=$token")

    companion object {
        /** A pairing code is six characters from the server's unambiguous alphabet. */
        fun looksLikeCode(input: String): Boolean = input.trim().replace("-", "").length == CODE_LEN

        private const val TAG = "PenwiseLibrary"
        private const val CODE_LEN = 6
        private const val TIMEOUT_MS = 4_000
        private const val HTTP_OK_FIRST = 200
        private const val HTTP_OK_LAST = 299
    }
}
