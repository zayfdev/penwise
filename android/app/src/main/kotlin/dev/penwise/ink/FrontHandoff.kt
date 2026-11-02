package dev.penwise.ink

/**
 * The hand-over of a committed stroke from the front-buffered layer (the pen's low-latency surface, above the window)
 * to the in-window dried ink ([DryInkView]). The front layer is hidden in a transaction that goes out with the window
 * frame that first draws the committed stroke, so no frame shows the stroke twice or not at all. Once that
 * transaction landed the front buffer is cleared, unless a new stroke has been drawn on it meanwhile, which then
 * needs the layer shown again. Several hand-overs can be on their way at once (quick strokes); only the last one
 * landing decides. Plain state, UI thread.
 */
internal class FrontHandoff {
    /** What to do with the front layer once a hide has landed. */
    enum class Landed {
        /** Another hide is still on its way: nothing yet. */
        WAIT,

        /** Nothing new was drawn: clear the front buffer (it is hidden already). */
        CLEAR,

        /** A new stroke is on the front buffer: show the layer again. */
        SHOW,
    }

    private var inFlight = 0
    private var drawnSince = false

    /** A hide is on its way. */
    val pending: Boolean get() = inFlight > 0

    /** A stroke was committed: its hide goes out with the next window frame. */
    fun committed() {
        inFlight++
        drawnSince = false
    }

    /** The front buffer drew a frame (a new stroke) while a hide may be on its way. */
    fun frontDrawn() {
        if (pending) drawnSince = true
    }

    /** The front buffer's content was abandoned (a cancelled gesture): nothing on it needs showing. */
    fun abandoned() {
        drawnSince = false
    }

    /** A hide landed. */
    fun landed(): Landed {
        if (inFlight == 0) return Landed.WAIT
        inFlight--
        if (inFlight > 0) return Landed.WAIT
        return if (drawnSince) Landed.SHOW else Landed.CLEAR
    }

    /** The surface or the renderer went away: nothing in flight will land. */
    fun reset() {
        inFlight = 0
        drawnSince = false
    }
}
