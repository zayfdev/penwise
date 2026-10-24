package dev.penwise.tools

/** The eight swatch colors (RGB, opaque). */
internal object Palette {
    const val INK_BLACK = 0xFF1A1A1A.toInt()
    const val GRAPHITE = 0xFF6B6B6B.toInt()
    const val BLUE = 0xFF1F5FD6.toInt()
    const val RED = 0xFFD62F2F.toInt()
    const val GREEN = 0xFF2E9E4F.toInt()
    const val ORANGE = 0xFFF08A1C.toInt()
    const val PURPLE = 0xFF7B3FD1.toInt()
    const val YELLOW = 0xFFF5D200.toInt()

    val swatches = listOf(INK_BLACK, GRAPHITE, BLUE, RED, GREEN, ORANGE, PURPLE, YELLOW)
}
