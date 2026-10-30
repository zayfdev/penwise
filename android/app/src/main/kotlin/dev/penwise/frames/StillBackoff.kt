package dev.penwise.frames

/**
 * How long a frame's still is left alone after the server answered `503` (it is capturing the page and will say so with
 * `Retry-After`). The still already on screen stays; the poll simply skips the frame until the time is up.
 */
internal class StillBackoff {
    private val until = HashMap<String, Long>()

    /** True while [frameId] must not be asked at [now]. */
    fun blocked(
        frameId: String,
        now: Long,
    ): Boolean = (until[frameId] ?: Long.MIN_VALUE) > now

    /** The server asked for [retryAfterMs] of quiet for [frameId], from [now]. */
    fun busy(
        frameId: String,
        now: Long,
        retryAfterMs: Long,
    ) {
        until[frameId] = now + retryAfterMs
    }

    fun clear(frameId: String) {
        until.remove(frameId)
    }

    companion object {
        /** Used when the header is missing or unreadable. */
        const val DEFAULT_MS = 2_000L
        private const val MIN_MS = 500L
        private const val MAX_MS = 30_000L
        private const val MS_PER_SECOND = 1_000L

        /** `Retry-After: <seconds>` in milliseconds, kept between half a second and half a minute. */
        fun retryAfterMs(header: String?): Long {
            val seconds = header?.trim()?.toLongOrNull()
            return (seconds?.times(MS_PER_SECOND) ?: DEFAULT_MS).coerceIn(MIN_MS, MAX_MS)
        }
    }
}
