package dev.penwise.frames

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update

/**
 * A frame to open on the canvas: a URL shown at a size preset, placed at a page position. The size is the page's CSS
 * width and height — the layer multiplies by the display density, so the page's breakpoints see the preset and the
 * canvas sees device px.
 */
data class FrameSpec(
    val id: String,
    val url: String,
    val x: Float,
    val y: Float,
    val cssWidth: Int,
    val cssHeight: Int,
) {
    companion object {
        /** The one debug frame (M0): a relaunch replaces it instead of opening another. */
        const val DEBUG_ID = "debug"

        /** Where the debug frame sits on the page (§M0: page position (100, 100)). */
        const val DEBUG_X = 100f
        const val DEBUG_Y = 100f

        fun debug(
            url: String,
            cssWidth: Int,
            cssHeight: Int,
        ): FrameSpec = FrameSpec(DEBUG_ID, url, DEBUG_X, DEBUG_Y, cssWidth, cssHeight)
    }
}

/** What the page inside a frame reports through the bridge. */
data class FrameScroll(
    val x: Int,
    val y: Int,
    val docWidth: Int,
    val docHeight: Int,
) {
    companion object {
        val NONE = FrameScroll(0, 0, 0, 0)
    }
}

/**
 * The frames of the open canvas, held in memory: M0 gets one from the debug launch intent. M1 turns frames into synced
 * items and this becomes their client-side projection.
 */
class FrameHold {
    private val state = MutableStateFlow<List<FrameSpec>>(emptyList())

    val frames: StateFlow<List<FrameSpec>> = state.asStateFlow()

    /** Adds [spec], replacing a frame with the same id. */
    fun add(spec: FrameSpec) {
        state.update { current -> current.filterNot { it.id == spec.id } + spec }
    }

    fun clear() {
        state.update { emptyList() }
    }
}
