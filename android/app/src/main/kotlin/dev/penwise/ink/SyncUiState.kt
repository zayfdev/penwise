package dev.penwise.ink

import dev.penwise.sync.DeadLetter
import dev.penwise.sync.Exports
import dev.penwise.sync.Outcome
import dev.penwise.sync.SyncEngine
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.io.OutputStream

/** What the sync dialog shows beside the connection state. */
data class SyncInfo(
    val pending: Int = 0,
    val deadLetters: List<DeadLetter> = emptyList(),
    val stateFailing: Boolean = false,
)

/**
 * The sync layer's health and decisions as the UI sees them: a failing state file, state files kept aside at
 * start, the open "different server" question, and the refused ops. Everything that takes the core lock runs on
 * [dispatcher], never the UI thread.
 */
class SyncUiState(
    private val dispatcher: CoroutineDispatcher,
    private val engine: () -> SyncEngine?,
) {
    /** True while the sync state file cannot be written (disk full): drawn ink is not safe on the tablet yet. */
    val stateFailing = MutableStateFlow(false)

    /** Names of unreadable state files kept aside at start; the UI says so until [dismissRecovery]. */
    val recovered = MutableStateFlow<List<String>>(emptyList())

    /** Why the sync layer could not open (the saved state could neither be read nor kept); `null` when fine. */
    val startupError = MutableStateFlow<String?>(null)

    /** True while the server named a different identity and the user has to decide about the tablet's ink. */
    val serverChanged = MutableStateFlow(false)

    /** The reason the last drawing change was refused (too large to save); `null` once dismissed. */
    val editRefused = MutableStateFlow<String?>(null)

    /** Hides the notice about kept-aside state files. */
    fun dismissRecovery() {
        recovered.value = emptyList()
    }

    /** What the sync dialog shows; read off the UI thread because it takes the core lock. */
    suspend fun info(): SyncInfo =
        withContext(dispatcher) {
            val open = engine() ?: return@withContext SyncInfo()
            SyncInfo(open.core.outbox().pending, open.core.deadLetters(), open.stateFailing)
        }

    /** Writes the refused ops to [out] (a file the user chose); returns how many. */
    suspend fun exportDeadLetters(out: OutputStream): Int =
        withContext(dispatcher) { Exports.deadLetters(requireNotNull(engine()).core, out) }

    /** Forgets every refused op (after the user exported them). */
    suspend fun clearDeadLetters() {
        withContext(dispatcher) {
            val core = requireNotNull(engine()).core
            core.deadLetters().forEach { core.removeDeadLetter(it.opId) }
        }
    }

    /** Writes the whole sync state to [out]: the tablet's last full copy, offered before ink is dropped. */
    suspend fun exportState(out: OutputStream) {
        withContext(dispatcher) { Exports.state(requireNotNull(engine()).core, out) }
    }

    /** Answers "the server is a different one": keep the ink (re-send it) or drop it; [applied] gets the outcome. */
    fun resolveServerChange(
        scope: CoroutineScope,
        keep: Boolean,
        applied: (Outcome) -> Unit,
    ) {
        scope.launch(dispatcher) {
            val outcome = engine()?.resolveServerChange(keep) ?: return@launch
            serverChanged.value = false
            applied(outcome)
        }
    }
}
