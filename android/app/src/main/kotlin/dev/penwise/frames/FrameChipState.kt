package dev.penwise.frames

/**
 * What one frame's canvas controls show (P6): the Ghost toggle for a frame with a patch. Pure, so the states are
 * unit-tested without Compose. A variant's label, check and "Use this" are the [SwitcherState]'s.
 *
 * [ghostOn] is the owner's local before/after toggle: a frame without a patch never shows the chip, so it stays true.
 */
internal data class FrameChipState(
    val ghost: Boolean,
    val ghostOn: Boolean,
) {
    companion object {
        fun of(
            spec: FrameSpec,
            ghostOn: Boolean,
        ): FrameChipState {
            val ghost = spec.patch != null
            return FrameChipState(ghost = ghost, ghostOn = ghost && ghostOn)
        }
    }
}
