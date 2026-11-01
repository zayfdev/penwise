package dev.penwise.overview

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.async
import kotlinx.coroutines.withTimeoutOrNull

/** How a card's picture gets on screen. Pure of Android, so the order and the time limit are unit-tested. */
internal object CardLoading {
    /**
     * Shows the picture in memory at once ([peek]), then fetches the current one with [load] and shows that too: a
     * frame the agent changed must not keep its old picture (strips are content-addressed, so a fetch of an
     * unchanged one is cheap).
     */
    suspend fun <T : Any> showThenRefresh(
        peek: () -> T?,
        load: suspend () -> T?,
        show: (T) -> Unit,
    ) {
        peek()?.let(show)
        load()?.let(show)
    }

    /**
     * [fetch] started in [scope] (not in the caller: it may block on a socket for seconds and cannot be interrupted),
     * waited for at most [timeoutMs]. When it is late or fails, [fallback] (the picture in memory) is returned and the
     * fetch goes on in the background, to fill the cache.
     */
    suspend fun <T : Any> within(
        scope: CoroutineScope,
        timeoutMs: Long,
        fallback: () -> T?,
        fetch: suspend () -> T?,
    ): T? {
        val job = scope.async { fetch() }
        return withTimeoutOrNull(timeoutMs) { job.await() } ?: fallback()
    }
}
