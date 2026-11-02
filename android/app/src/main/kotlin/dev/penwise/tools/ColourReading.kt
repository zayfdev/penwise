package dev.penwise.tools

import dev.penwise.R

/**
 * What the agent reads a colour as: the core's answer (`penwise-core` `marks::colour`, one of `problem`, `idea`,
 * `good`, `none`) turned into the flyout's wording. The hue maths lives in the core only.
 */
internal enum class ColourReading(
    val label: Int,
) {
    PROBLEM(R.string.colour_problem),
    IDEA(R.string.colour_idea),
    KEEP(R.string.colour_keep),
    NOTE(R.string.colour_note),
    ;

    companion object {
        /** The reading for a core name; anything unknown is a plain note. */
        fun of(name: String): ColourReading =
            when (name) {
                "problem" -> PROBLEM
                "idea" -> IDEA
                "good" -> KEEP
                else -> NOTE
            }
    }
}
