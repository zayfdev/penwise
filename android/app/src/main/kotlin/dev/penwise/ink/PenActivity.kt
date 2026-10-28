package dev.penwise.ink

import android.os.SystemClock

/**
 * Whether the pen is on the glass or close to it, shared between the canvas (which sees the pen) and the screens
 * that must not act while a hand is writing (Back, retiring a page's scene). Written on the UI thread, read anywhere.
 */
class PenActivity {
    /** True from pen-down to pen-up. */
    @Volatile
    var drawing = false

    /** Until when (uptime ms) a hovering pen still counts as near. */
    @Volatile
    var nearUntil = 0L

    /** True while the pen draws or hovers. */
    fun active(now: Long = SystemClock.uptimeMillis()): Boolean = drawing || now < nearUntil
}
