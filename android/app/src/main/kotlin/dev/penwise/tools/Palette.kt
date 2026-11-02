package dev.penwise.tools

/**
 * The ink colours with a fixed place in the tools (RGB, opaque).
 *
 * [RED], [BLUE] and [GREEN] are the pinned meanings the agent reads (red = problem, blue = idea, green = keep). The
 * server centres its hue bands (15 degrees either side) on exactly these colours (`penwise-core` `marks::colour`), so
 * they must not drift. Every other colour is a plain note to the agent.
 */
internal object Palette {
    const val INK_BLACK = 0xFF1A1A1A.toInt()
    const val GRAPHITE = 0xFF6B6B6B.toInt()
    const val BLUE = 0xFF1F5FD6.toInt()
    const val RED = 0xFFD62F2F.toInt()
    const val GREEN = 0xFF2E9E4F.toInt()
    const val ORANGE = 0xFFF08A1C.toInt()
    const val PURPLE = 0xFF7B3FD1.toInt()
    const val YELLOW = 0xFFF5D200.toInt()
    const val TEAL = 0xFF0F9D9A.toInt()
    const val PINK = 0xFFE0457B.toInt()

    /** The pinned meaning swatches, in the order the colour flyout lists them: problem, idea, keep. */
    val pinned = listOf(RED, BLUE, GREEN)

    /** Ink black and graphite: one tap, beside the pinned meanings, and never a recent. */
    val inkPins = listOf(INK_BLACK, GRAPHITE)

    /** What the recent-colours row holds before the owner has picked anything: plain-note colours, newest first. */
    val starterRecents = listOf(ORANGE, PURPLE, YELLOW, TEAL, PINK)

    private const val DARK_INK_LUMA = 0.12
    private const val R_W = 0.2126
    private const val G_W = 0.7152
    private const val B_W = 0.0722

    /** True for ink so dark (black, graphite shades) that its swatch needs an outline to show on a dark panel. */
    fun isDarkInk(rgb: Int): Boolean {
        val r = ((rgb shr 16) and 0xFF) / 255.0
        val g = ((rgb shr 8) and 0xFF) / 255.0
        val b = (rgb and 0xFF) / 255.0
        return R_W * r + G_W * g + B_W * b < DARK_INK_LUMA
    }
}
