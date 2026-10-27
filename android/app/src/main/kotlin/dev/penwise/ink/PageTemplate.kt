package dev.penwise.ink

/** A page template the new-page dialog offers. */
enum class PageTemplate(
    val label: String,
    val hint: String,
    val background: String,
    val infinite: Boolean,
) {
    Blank("Blank", "plain A4", "blank", false),
    Ruled("Ruled", "lines", "ruled", false),
    Grid("Grid", "squares", "grid", false),
    Infinite("Infinite", "unbounded", "blank", true),
}
