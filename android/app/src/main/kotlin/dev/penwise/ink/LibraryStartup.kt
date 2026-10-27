package dev.penwise.ink

import android.util.Log
import dev.penwise.sync.LegacyResult
import dev.penwise.sync.SyncEngine
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withTimeoutOrNull

private const val TAG = "PenwiseSync"
private const val FIRST_SYNC_TIMEOUT_MS = 4_000L

/**
 * First-run work on the library: wait briefly for the server when the catalog could still gain content from it,
 * seed a default notebook, and import the pre-rewrite page files once the replica is *really* caught up.
 *
 * @param onImported called after a finished import so the catalog is shown again
 */
internal class LibraryStartup(
    private val scope: CoroutineScope,
    private val onImported: () -> Unit,
) {
    /** Completes when the first sync is over for good or ill (caught up, or the server is unreachable). */
    private val firstContact = CompletableDeferred<Unit>()

    /** Completes only when the core says the replica holds what the server announced. */
    private val caughtUp = CompletableDeferred<Unit>()

    /** The replica is caught up (`SyncListener.onCaughtUp`). */
    fun caughtUp() {
        caughtUp.complete(Unit)
        firstContact.complete(Unit)
    }

    /** No server answered (`SyncListener.onUnreachable`). */
    fun unreachable() {
        firstContact.complete(Unit)
    }

    /** Waits (bounded) for the first contact, seeds a default library, and schedules the legacy import. */
    suspend fun prepare(open: SyncEngine) {
        val empty =
            open.library
                .list(null)
                .all()
                .isEmpty()
        val importing = open.legacy.pending()
        if (empty || importing) withTimeoutOrNull(FIRST_SYNC_TIMEOUT_MS) { firstContact.await() }
        if (open.library
                .list(null)
                .all()
                .isEmpty()
        ) {
            open.library.createDefault()
        }
        if (importing) {
            // No timeout: on a timeout or an unreachable server the import stays pending until a real catch-up
            // (this run, once the server answers) or the next launch.
            scope.launch(Dispatchers.IO) {
                caughtUp.await()
                importLegacy(open)
            }
        }
    }

    // Native errors and panics surface as RuntimeException.
    @Suppress("TooGenericExceptionCaught")
    private fun importLegacy(open: SyncEngine) {
        try {
            val result = open.legacy.runWhenCaughtUp(open.core, open::flush)
            Log.i(TAG, "legacy import: $result")
            if (result == LegacyResult.DONE) onImported()
        } catch (error: RuntimeException) {
            // Not marked done: the next launch retries (everything dedupes by deterministic id).
            Log.e(TAG, "legacy import failed: ${error.message}")
        }
    }
}
