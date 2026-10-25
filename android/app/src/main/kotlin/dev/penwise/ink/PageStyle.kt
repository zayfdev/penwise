package dev.penwise.ink

/** What paper the current page is: the pattern the tablet draws under the ink, and the sheet's size. */
data class PageStyle(
    val background: String,
    val widthPx: Float,
    val heightPx: Float,
    val infinite: Boolean,
) {
    companion object {
        /** Space between rules, grid lines and dots, in page px (7 mm at 340 dpi; the server's constant). */
        const val SPACING_PX = 93.7f

        /** The server's default sheet, landscape, which is the tablet's own size. */
        const val DEFAULT_WIDTH_PX = 2560f

        /** The server's default sheet height. */
        const val DEFAULT_HEIGHT_PX = 1600f

        val BLANK = PageStyle("blank", DEFAULT_WIDTH_PX, DEFAULT_HEIGHT_PX, infinite = false)

        /** Style for a page, falling back to the default sheet when it is unknown. */
        fun of(page: PageMeta?): PageStyle =
            page?.let {
                PageStyle(
                    background = it.background,
                    widthPx = if (it.infinite) 0f else DEFAULT_WIDTH_PX,
                    heightPx = if (it.infinite) 0f else DEFAULT_HEIGHT_PX,
                    infinite = it.infinite,
                )
            } ?: BLANK
    }
}
