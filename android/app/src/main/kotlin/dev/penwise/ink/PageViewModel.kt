package dev.penwise.ink

import androidx.lifecycle.ViewModel
import dev.penwise.tools.ToolState

/** Page and tool state that survives configuration changes. */
class PageViewModel : ViewModel() {
    val store = StrokeStore()
    val tools = ToolState()
}
