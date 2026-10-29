package dev.penwise.frames

/**
 * What one frame's canvas controls show (P6): the Ghost toggle for a frame with a patch, and the label, "Use this"
 * and dimmed state of a variant frame. Pure, so the states are unit-tested without Compose.
 *
 * [ghostOn] is the owner's local before/after toggle: a frame without a patch never shows the chip, so it stays true.
 */
internal data class FrameChipState(
    val ghost: Boolean,
    val ghostOn: Boolean,
    val label: String,
    val useThis: Boolean,
    val chosen: Boolean,
    val dimmed: Boolean,
) {
    companion object {
        fun of(
            spec: FrameSpec,
            ghostOn: Boolean,
        ): FrameChipState {
            val ghost = spec.patch != null
            return FrameChipState(
                ghost = ghost,
                ghostOn = ghost && ghostOn,
                label = if (spec.group == null) "" else spec.label,
                useThis = spec.group != null && !spec.chosen,
                chosen = spec.chosen,
                dimmed = isDimmed(spec),
            )
        }
    }
}

/** A variant the owner did not pick: dimmed so the chosen one reads as the answer. */
internal fun isDimmed(spec: FrameSpec): Boolean = spec.group != null && !spec.chosen
