package dev.penwise.ink

import android.graphics.RectF
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.setValue

/**
 * The items the lasso selected, as the toolbar and the canvas see them: [count] drives the toolbar's delete button,
 * [ids] (16 bytes each) is what the canvas moves or deletes. Set and read on the UI thread.
 */
class Selection {
    var count by mutableIntStateOf(0)
        private set

    var ids: ByteArray = ByteArray(0)
        private set

    /** Page-space frame around the selection, kept by the canvas. */
    internal val box = RectF()

    /** Registered by the canvas; runs the delete as one undo step. */
    internal var onDelete: (() -> Unit)? = null

    /** Registered by the canvas; runs when a non-empty selection is cleared, to wipe its frame off the front buffer. */
    internal var onCleared: (() -> Unit)? = null

    fun select(ids: ByteArray) {
        this.ids = ids
        count = ids.size / ItemKey.SIZE
    }

    fun clear() {
        val framed = count > 0
        select(ByteArray(0))
        box.setEmpty()
        if (framed) onCleared?.invoke()
    }

    /** Toolbar action: delete the selected items. */
    fun requestDelete() {
        onDelete?.invoke()
    }
}
