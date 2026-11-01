package dev.penwise.frames

/**
 * Which frame a board opens on: the camera fits it to the screen's width, so the owner starts inside a page the way
 * a browser opens one, never at the board's raw scale with the page in a corner. The top lane comes first (lanes
 * stack downward), and within it the leftmost frame that is not an agent's variant (variants sit to its right).
 */
internal object FrameOpening {
    fun target(specs: List<FrameSpec>): String? {
        val frames = specs.filter { it.group == null }.ifEmpty { specs }
        return frames.minWithOrNull(compareBy<FrameSpec>({ it.y }, { it.x }))?.id
    }
}
