package dev.penwise.frames

/**
 * The size presets of a frame (§12.4): the page inside is laid out at the preset's CSS size, so the site's breakpoints
 * see the preset at any canvas zoom. `custom` is a frame the user sized otherwise; it keeps whatever size it has.
 */
@Suppress("MagicNumber") // the contract's preset sizes (C1)
enum class FramePreset(
    val key: String,
    val cssWidth: Int,
    val cssHeight: Int,
) {
    PHONE("phone", 390, 844),
    TABLET("tablet", 834, 1194),
    DESKTOP("desktop", 1440, 900),
    ;

    /** The preset a chip's switch goes to next: phone → tablet → desktop → phone. */
    fun next(): FramePreset = entries[(ordinal + 1) % entries.size]

    companion object {
        /** The preset of a frame whose size was set by hand. */
        const val CUSTOM = "custom"

        fun byKey(key: String): FramePreset? = entries.firstOrNull { it.key == key }

        /** The preset that follows [key]; an unknown or custom frame starts again at phone. */
        fun nextFrom(key: String): FramePreset = byKey(key)?.next() ?: PHONE
    }
}
