package dev.penwise.ink

/** One edit the mirror can apply without a full page resend. */
sealed interface PageOp {
    /** New strokes (agent marks merged remotely never produce this). */
    data class Add(
        val strokes: List<SavedStroke>,
    ) : PageOp

    /** Strokes removed by seed. */
    data class Remove(
        val seeds: List<Long>,
    ) : PageOp

    /** Erased strokes swapped for their fragments. */
    data class Replace(
        val seeds: List<Long>,
        val fragments: List<SavedStroke>,
    ) : PageOp

    /** Everything gone. */
    data object Clear : PageOp
}
