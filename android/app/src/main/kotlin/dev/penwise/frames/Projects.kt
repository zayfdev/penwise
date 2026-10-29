package dev.penwise.frames

import android.util.Log
import org.json.JSONArray
import org.json.JSONException
import org.json.JSONObject
import java.io.IOException
import java.net.HttpURLConnection
import java.net.URL

/** A project linked with `penwise link`: a folder the server serves, on its own port (§4, C2). */
data class DesignProject(
    val id: String,
    val name: String,
    val root: String,
    val kind: String,
    val port: Int,
) {
    /** What a frame of this project goes through: its port on the host the tablet syncs with. */
    val isStatic: Boolean get() = kind == KIND_STATIC

    private companion object {
        const val KIND_STATIC = "static"
    }
}

/** The `GET /projects` reply, parsed. Pure JSON through `org.json`, so it is unit-tested. */
internal object ProjectList {
    fun parse(json: String): List<DesignProject> =
        try {
            val array = JSONArray(json)
            (0 until array.length()).mapNotNull { array.optJSONObject(it)?.let(::project) }
        } catch (ignored: JSONException) {
            emptyList()
        }

    private fun project(json: JSONObject): DesignProject? {
        val id = json.optString(ID)
        val port = json.optInt(PORT)
        if (id.isEmpty() || port <= 0) return null
        return DesignProject(
            id = id,
            name = json.optString(NAME, id),
            root = json.optString(ROOT),
            kind = json.optString(KIND),
            port = port,
        )
    }

    private const val ID = "id"
    private const val NAME = "name"
    private const val ROOT = "root"
    private const val KIND = "kind"
    private const val PORT = "port"
}

/**
 * `GET /projects` from the PC server (C2). The list is small and changes only when a project is linked, so the tablet
 * caches it and asks again on reconnect.
 */
internal class ProjectsClient(
    private val httpBase: String,
    private val token: String,
) {
    /** The linked projects; `null` when the server is away or the token is refused (the cache then stays). */
    fun list(): List<DesignProject>? {
        val connection =
            runCatching { URL(httpBase.trimEnd('/') + PROJECTS).openConnection() as HttpURLConnection }
                .getOrNull() ?: return null
        return try {
            connection.requestMethod = "GET"
            connection.connectTimeout = TIMEOUT_MS
            connection.readTimeout = TIMEOUT_MS
            connection.setRequestProperty("Accept", "application/json")
            if (token.isNotEmpty()) connection.setRequestProperty("X-Penwise-Token", token)
            val code = connection.responseCode
            if (code !in HTTP_OK_FIRST..HTTP_OK_LAST) {
                Log.w(TAG, "GET $PROJECTS -> $code")
                return null
            }
            val body = connection.inputStream.use { it.readBytes().decodeToString() }
            ProjectList.parse(body)
        } catch (error: IOException) {
            Log.w(TAG, "GET $PROJECTS failed: ${error.message}")
            null
        } finally {
            connection.disconnect()
        }
    }

    private companion object {
        const val TAG = "PenwiseFrames"
        const val PROJECTS = "/projects"
        const val TIMEOUT_MS = 4_000
        const val HTTP_OK_FIRST = 200
        const val HTTP_OK_LAST = 299
    }
}
