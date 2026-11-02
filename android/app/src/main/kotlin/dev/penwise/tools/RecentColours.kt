package dev.penwise.tools

import android.content.Context
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.core.content.edit

/**
 * The last colours the owner picked from the wheel, newest first. A colour already in the list moves to the front,
 * the colours with their own buttons ([Palette.pinned], [Palette.inkPins]) never enter it, and the list keeps at most
 * [limit]. [onChange] sees every new list, so it can be saved.
 */
internal class RecentColours(
    start: List<Int> = Palette.starterRecents,
    private val limit: Int = LIMIT,
    private val onChange: (List<Int>) -> Unit = {},
) {
    var items: List<Int> by mutableStateOf(start.map { it and RGB_MASK }.take(limit))
        private set

    /** Records [rgb] (opaque RGB) as the newest pick. */
    fun remember(rgb: Int) {
        val colour = rgb and RGB_MASK
        if ((Palette.pinned + Palette.inkPins).any { (it and RGB_MASK) == colour }) return
        items = (listOf(colour) + items.filter { (it and RGB_MASK) != colour }).take(limit)
        onChange(items)
    }

    companion object {
        private const val LIMIT = 5
        private const val RGB_MASK = 0xFFFFFF
        private const val PREFS = "penwise_tools"
        private const val KEY = "recent_colours"
        private const val HEX_RADIX = 16

        /** `RRGGBB,RRGGBB,...` for saving. */
        fun encode(items: List<Int>): String = items.joinToString(",") { "%06X".format(it and RGB_MASK) }

        /** The list [encode] wrote; anything unreadable is skipped, and null or blank text is no list. */
        fun decode(text: String?): List<Int>? {
            if (text.isNullOrBlank()) return null
            return text.split(",").mapNotNull { it.trim().toIntOrNull(HEX_RADIX) }.map { it and RGB_MASK }
        }

        /** Recents that survive the process: loaded from, and saved to, the app's preferences. */
        fun persisted(context: Context): RecentColours {
            val prefs = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            val saved = decode(prefs.getString(KEY, null))
            return RecentColours(saved ?: Palette.starterRecents) { prefs.edit { putString(KEY, encode(it)) } }
        }
    }
}
