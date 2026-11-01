package dev.penwise.frames

import dev.penwise.ink.HitAnswer
import dev.penwise.ink.PageScene
import dev.penwise.sync.Ids

/** What a pen-down asks of the layer: the frame under the nib, the layouts, the core's frame space, the server. */
internal class LayerPenHost(
    private val lookup: FrameLookup,
    private val stills: FrameStills,
    private val scene: () -> PageScene?,
) : PenHitHost {
    override fun liveAt(
        x: Float,
        y: Float,
    ): FrameEntry? = lookup.liveAt(x, y)

    override fun specAt(
        x: Float,
        y: Float,
    ): FrameSpec? = lookup.specAt(x, y)

    override fun boardX(windowX: Float): Float = lookup.boardX(windowX)

    override fun boardY(windowY: Float): Float = lookup.boardY(windowY)

    override fun layoutOf(frameId: String): FrameMessage.Layout? = stills.layouts[frameId]

    override fun docPoint(
        frameId: String,
        boardX: Float,
        boardY: Float,
    ): FloatArray? = scene()?.frameDocPoint(Ids.bytes(frameId) ?: return null, boardX, boardY)

    override fun askServer(
        frameId: String,
        docX: Float,
        docY: Float,
        done: (HitAnswer?) -> Unit,
    ) = stills.hit(frameId, docX, docY, done)
}
