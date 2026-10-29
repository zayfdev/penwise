package dev.penwise.frames

/** What the layer must do to show [next] instead of [previous]: one bucket per kind of WebView work (§M1). */
data class FramePlan(
    val open: List<FrameSpec> = emptyList(),
    val load: List<FrameSpec> = emptyList(),
    val resize: List<FrameSpec> = emptyList(),
    val move: List<FrameSpec> = emptyList(),
    val close: List<String> = emptyList(),
) {
    val isEmpty: Boolean
        get() = open.isEmpty() && load.isEmpty() && resize.isEmpty() && move.isEmpty() && close.isEmpty()
}

/**
 * The difference between two frame lists, by item id: a new frame is opened, a frame whose URL changed is loaded
 * again, one whose CSS size changed is re-laid out, one that only moved is translated, and a frame that is gone is
 * closed. Kept free of Android types so the create/reload/update/destroy decisions are unit-tested.
 */
object FrameDiff {
    /**
     * Classifies [next] against [previous]. A frame can appear in one bucket per call, in the order of the problem:
     * load (a different URL, possibly a different project), then resize, then move.
     */
    fun plan(
        previous: List<FrameSpec>,
        next: List<FrameSpec>,
    ): FramePlan {
        val before = previous.associateBy { it.id }
        val after = next.mapTo(HashSet()) { it.id }
        val open = ArrayList<FrameSpec>()
        val load = ArrayList<FrameSpec>()
        val resize = ArrayList<FrameSpec>()
        val move = ArrayList<FrameSpec>()
        for (spec in next) {
            val old = before[spec.id]
            when {
                old == null -> open += spec
                old.url != spec.url -> load += spec
                old.cssWidth != spec.cssWidth || old.cssHeight != spec.cssHeight -> resize += spec
                old.x != spec.x || old.y != spec.y -> move += spec
            }
        }
        return FramePlan(
            open = open,
            load = load,
            resize = resize,
            move = move,
            close = previous.mapNotNull { if (it.id in after) null else it.id },
        )
    }
}
