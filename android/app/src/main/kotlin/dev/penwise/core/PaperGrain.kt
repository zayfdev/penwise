package dev.penwise.core

/** Paper grain texture shared with the server rasterizer (`stamp::texture`). */
object PaperGrain {
    private const val MAX_TILE_SIZE = 8192

    /** Bakes a `size`×`size` grayscale tile (row-major, one byte per pixel) at grain `scale`. */
    fun bakeTile(
        scale: Float,
        size: Int,
    ): ByteArray {
        require(size in 1..MAX_TILE_SIZE) { "size must be in 1..$MAX_TILE_SIZE, was $size" }
        return Native.bakePaperTile(scale, size)
    }
}
