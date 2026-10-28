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

    /**
     * `GET /assets/<id>` decoded as a bitmap of at most about [maxPixels] pixels (downsampled while decoding, so a
     * photo never costs its full size in memory); `null` when the server is away or the asset is gone.
     */
    fun asset(
        asset: String,
        maxPixels: Int,
    ): android.graphics.Bitmap? {
        val connection =
            runCatching { URL(base(httpBase(baseUrl), "/assets/$asset")).openConnection() as HttpURLConnection }
                .getOrNull() ?: return null
        return try {
            connection.requestMethod = "GET"
            connection.connectTimeout = TIMEOUT_MS
            connection.readTimeout = TIMEOUT_MS
            if (token.isNotEmpty()) connection.setRequestProperty("X-Penwise-Token", token)
            if (connection.responseCode !in HTTP_OK_FIRST..HTTP_OK_LAST) {
                Log.w(TAG, "GET /assets/$asset -> ${connection.responseCode}")
                null
            } else {
                decodeScaled(connection.inputStream.use { it.readBytes() }, maxPixels)
            }
        } catch (error: IOException) {
            Log.w(TAG, "GET /assets/$asset failed: ${error.message}")
            null
        } finally {
            connection.disconnect()
        }
    }

    private fun decodeScaled(
        bytes: ByteArray,
        maxPixels: Int,
    ): android.graphics.Bitmap? {
        val options =
            android.graphics.BitmapFactory
                .Options()
                .apply { inJustDecodeBounds = true }
        android.graphics.BitmapFactory.decodeByteArray(bytes, 0, bytes.size, options)
        options.inSampleSize = PageImages.sampleSize(options.outWidth, options.outHeight, maxPixels)
        options.inJustDecodeBounds = false
        return android.graphics.BitmapFactory.decodeByteArray(bytes, 0, bytes.size, options)
    }

    private fun send(
        method: String,
        path: String,
        body: String?,
    ): String? {
        val connection =
            runCatching { URL(base(httpBase(baseUrl), path)).openConnection() as HttpURLConnection }
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

    private fun base(
        root: String,
        path: String,
    ): String = root.trimEnd('/') + path + (if (token.isEmpty()) "" else "?token=$token")

    companion object {
        /**
         * `ws://host:port/sync` becomes the `http://host:port` base; a bare `host:port` gets `http://`; extra query
         * parts are dropped. Whatever was typed, the result always carries a scheme, so building a URL never throws.
         */
        internal fun httpBase(typed: String): String {
            val url =
                typed
                    .trim()
                    .substringBefore('?')
                    .trimEnd('/')
                    .removeSuffix("/sync")
            return when {
                url.startsWith("wss://", ignoreCase = true) -> "https://" + url.substring("wss://".length)
                url.startsWith("ws://", ignoreCase = true) -> "http://" + url.substring("ws://".length)
                url.startsWith("http://", ignoreCase = true) || url.startsWith("https://", ignoreCase = true) -> url
                else -> "http://$url"
            }
        }

        /** A pairing code is six characters from the server's unambiguous alphabet. */
        fun looksLikeCode(input: String): Boolean = input.trim().replace("-", "").length == CODE_LEN

        private const val TAG = "PenwiseLibrary"
        private const val CODE_LEN = 6
        private const val TIMEOUT_MS = 4_000
        private const val HTTP_OK_FIRST = 200
        private const val HTTP_OK_LAST = 299
    }
}
