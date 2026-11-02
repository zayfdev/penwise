package dev.penwise.tools

import androidx.compose.runtime.Composable
import androidx.compose.runtime.MutableState
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember

/** The panels the dock opens above its buttons. */
internal enum class DockFlyout { PEN, ERASER, SHAPES, COLOUR, MORE }

/**
 * Which flyout is open: at most one. The "more" panel is the drawer, whose open state the screen owns ([moreOpen],
 * changed through [onMore]); the others are held here. Opening one closes the rest, and the same button closes it
 * again.
 */
internal class DockFlyouts(
    private val others: MutableState<DockFlyout?>,
    private val moreOpen: Boolean,
    private val onMore: (Boolean) -> Unit,
) {
    val open: DockFlyout?
        get() = if (moreOpen) DockFlyout.MORE else others.value

    fun isOpen(flyout: DockFlyout): Boolean = open == flyout

    fun toggle(flyout: DockFlyout) {
        if (open == flyout) {
            close()
            return
        }
        others.value = flyout.takeIf { it != DockFlyout.MORE }
        onMore(flyout == DockFlyout.MORE)
    }

    /** Back: closes the open flyout and says so, or leaves the screen to go back (false) when none is open. */
    fun handleBack(): Boolean {
        if (open == null) return false
        close()
        return true
    }

    /** Closes whatever is open (the pen touching the page does this). */
    fun close() {
        others.value = null
        if (moreOpen) onMore(false)
    }
}

/** The dock's flyouts for a screen whose drawer is [moreOpen]. */
@Composable
internal fun rememberDockFlyouts(
    moreOpen: Boolean,
    onMore: (Boolean) -> Unit,
): DockFlyouts {
    val others = remember { mutableStateOf<DockFlyout?>(null) }
    return DockFlyouts(others, moreOpen, onMore)
}
