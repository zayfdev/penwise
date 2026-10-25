package dev.penwise.ink

/**
 * Time gate for live previews sent over the network.
 *
 * Local rendering must follow every motion event, but a shape drag can produce a hundred whole-shape frames a
 * second; sending them all saturates the link and the mirror plays catch-up long after the pen lifts.
 */
internal class PreviewGate(
    private val minIntervalMs: Long,
) {
    private var lastMs = UNSET

    /** True when a preview may be sent now; records the time when it says yes. */
    fun allow(nowMs: Long): Boolean {
        if (lastMs != UNSET && nowMs - lastMs < minIntervalMs) return false
        lastMs = nowMs
        return true
    }

    /** Forgets the last send, so the next shaped frame goes out immediately (new gesture). */
    fun reset() {
        lastMs = UNSET
    }

    private companion object {
        /** `0` is a valid clock reading, so "never" needs a value of its own. */
        const val UNSET = Long.MIN_VALUE
    }
}
