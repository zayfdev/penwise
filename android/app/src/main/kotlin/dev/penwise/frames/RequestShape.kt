package dev.penwise.frames

/**
 * What a request carries. `POST` and `PUT` always send JSON: the given body, or `{}` when there is none. Without an
 * explicit type, Android's `HttpURLConnection` labels a bodiless POST `application/x-www-form-urlencoded`, which the
 * server rightly refuses. Other methods carry nothing.
 */
internal class RequestShape(
    val contentType: String,
    val body: ByteArray,
) {
    internal companion object {
        const val JSON = "application/json"
        private val EMPTY_OBJECT = "{}".toByteArray(Charsets.UTF_8)

        fun of(
            method: String,
            body: ByteArray?,
        ): RequestShape? =
            when (method) {
                "POST", "PUT" -> RequestShape(JSON, body ?: EMPTY_OBJECT)
                else -> body?.let { RequestShape(JSON, it) }
            }
    }
}
