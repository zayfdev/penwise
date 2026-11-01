package dev.penwise.overview

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.async
import kotlinx.coroutines.delay
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

    /** One try at a picture: ready, being made by the server (ask again after [Busy.retryAfterMs]), or not there. */
    sealed interface Attempt<out T> {
        data class Ready<T>(
            val value: T,
        ) : Attempt<T>

        data class Busy(
            val retryAfterMs: Long,
        ) : Attempt<Nothing>

        data object Failed : Attempt<Nothing>
    }

    /**
     * Tries [attempt] up to [maxTries] times, waiting the server's `Retry-After` between tries while it says busy; a
     * failure ends it. Cancelling the caller (the card leaving the screen) ends the wait.
     */
    suspend fun <T : Any> untilReady(
        maxTries: Int,
        wait: suspend (Long) -> Unit = { delay(it) },
        attempt: suspend () -> Attempt<T>,
    ): T? {
        repeat(maxTries) { n ->
            when (val result = attempt()) {
                is Attempt.Ready -> return result.value
                is Attempt.Failed -> return null
                is Attempt.Busy -> if (n < maxTries - 1) wait(result.retryAfterMs)
            }
        }
        return null
    }
}
