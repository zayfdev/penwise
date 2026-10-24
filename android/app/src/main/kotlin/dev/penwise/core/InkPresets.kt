package dev.penwise.core

/** The engine's brush presets, in `Preset::ALL` order (index = `presetIndex` of [InkStroke]). */
object InkPresets {
    /** Default `0xAARRGGBB` color per preset. */
    val defaultArgb: List<Int> = Native.presetDefaultArgb().toList()

    /** Stable preset names (e.g. `"ballpoint"`). */
    val names: List<String> = List(defaultArgb.size) { Native.presetName(it) }
}
