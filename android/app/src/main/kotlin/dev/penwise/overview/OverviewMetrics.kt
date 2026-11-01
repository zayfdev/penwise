package dev.penwise.overview

/** Sizes of a strip's cards and columns, in the unit of the screen given (dp or px). */
internal data class OverviewMetrics(
    val cardWidth: Float,
    val cardHeight: Float,
    val gutter: Float,
    /** Padding either side of the row: with it the focused column is centred and its neighbours peek in. */
    val sidePadding: Float,
) {
    /** How much of a neighbouring column shows at the screen's edge. */
    val peek: Float get() = sidePadding - gutter

    companion object {
        private const val HEIGHT_SHARE = 0.46f
        private const val WIDTH_SHARE_MAX = 0.58f
        private const val GUTTER_SHARE = 0.02f

        /**
         * Cards for a screen [screenWidth] by [screenHeight] and frames of [aspect] (width / height): the card is about
         * half the screen tall, but never wider than 58% of the screen, so a neighbour always peeks.
         */
        fun of(
            screenWidth: Float,
            screenHeight: Float,
            aspect: Float,
        ): OverviewMetrics {
            val ratio = aspect.takeIf { it > 0f } ?: 1f
            val width = minOf(screenHeight * HEIGHT_SHARE * ratio, screenWidth * WIDTH_SHARE_MAX)
            val gutter = screenWidth * GUTTER_SHARE
            return OverviewMetrics(width, width / ratio, gutter, (screenWidth - width) / 2f)
        }
    }
}

/** Which column the strip shows after its list changed. */
internal object SnapIndex {
    /**
     * The index of [key] among [keys], so a column list that gains past generations on the left keeps the owner on the
     * column they were on; [fallback] (clamped) when it is gone.
     */
    fun after(
        key: String?,
        keys: List<String>,
        fallback: Int,
    ): Int = keys.indexOf(key).takeIf { it >= 0 } ?: fallback.coerceIn(0, (keys.size - 1).coerceAtLeast(0))
}

/** How long the overview's motions take (contract §3.3: zoom at most 260 ms, deck at most 220 ms). */
internal object OverviewMotion {
    const val ZOOM_MS = 240
    const val DECK_MS = 200
    const val FADE_MS = 140

    /** The zoom-in from a card, or a crossfade when the owner reduced motion. */
    fun zoom(reduced: Boolean): Motion = if (reduced) Motion.CROSSFADE else Motion.ZOOM

    enum class Motion { ZOOM, CROSSFADE }
}
