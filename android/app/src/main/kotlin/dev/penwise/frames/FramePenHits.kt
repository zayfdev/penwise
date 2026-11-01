package dev.penwise.frames

import dev.penwise.ink.HitAnswer
import dev.penwise.ink.PageScene
import dev.penwise.ink.PendingBinding
import dev.penwise.sync.Ids

/** What a pen-down needs from the layer to ask the right party what is under the nib. */
internal interface PenHitHost {
    /** The live page when the window point is on its window. */
    fun liveAt(
        x: Float,
        y: Float,
    ): FrameEntry?

    /** The frame whose box holds the window point (topmost), or null. */
    fun specAt(
        x: Float,
        y: Float,
    ): FrameSpec?

    /** The board point under a window point. */
    fun boardX(windowX: Float): Float

    fun boardY(windowY: Float): Float

    /** The last layout of a frame (the live page's, or its still's). */
    fun layoutOf(frameId: String): FrameMessage.Layout?

    /** The frame's document point under a board point (the core's frame space); null when unknown. */
    fun docPoint(
        frameId: String,
        boardX: Float,
        boardY: Float,
    ): FloatArray?

    /** The server's hit for a document point; [done] on the main thread. */
    fun askServer(
        frameId: String,
        docX: Float,
        docY: Float,
        done: (HitAnswer?) -> Unit,
    )
}

/**
 * A pen-down at a window point (K3): on the live window the page's bridge answers (`pw:hit?`, viewport px); anywhere
 * else on a frame's box the server does (`GET /frames/{id}/hit`, document px). The answer is read when the stroke
 * commits; no answer by then means an area binding. A pen-down off every frame has no binding at all.
 */
internal class FramePenHits(
    private val host: PenHitHost,
) {
    private val hits = FrameHits()

    fun begin(
        windowX: Float,
        windowY: Float,
        bindToElement: Boolean,
    ): PendingBinding? {
        host.liveAt(windowX, windowY)?.let { entry ->
            val x = entry.placement.toCssX(windowX, entry.pxPerCss)
            val y = entry.placement.toCssY(windowY, entry.pxPerCss)
            return binding(entry.spec, bindToElement, hits.ask(entry.view, x, y))
        }
        val spec = host.specAt(windowX, windowY) ?: return null
        val id = hits.reserve()
        val doc = host.docPoint(spec.id, host.boardX(windowX), host.boardY(windowY))
        if (doc != null) host.askServer(spec.id, doc[0], doc[1]) { answer -> hits.answer(id, answer) }
        return binding(spec, bindToElement, id)
    }

    /** The live page's answer to a hit request. */
    fun answered(
        id: Int,
        key: String,
        rect: List<Float>,
    ) = hits.answer(id, key, rect)

    fun clear() = hits.clear()

    private fun binding(
        spec: FrameSpec,
        bindToElement: Boolean,
        id: Int,
    ) = PendingBinding(
        frame = spec.id,
        bindToElement = bindToElement,
        answer = { hits.take(id) },
        prime = { scene, hit -> prime(spec, scene, hit) },
        onDone = { hits.forget(id) },
    )

    /**
     * The hit's element may be one the page never watched, so the core lacks its rect. Merge the rect just measured
     * into the frame's last layout before the stroke commits (M2Core's first-draw flow). The server's rect is in
     * document px; the core's merge takes viewport px, so the layout's scroll comes off.
     */
    private fun prime(
        spec: FrameSpec,
        scene: PageScene,
        hit: HitAnswer,
    ) {
        val layout = host.layoutOf(spec.id) ?: return
        val offsetX = if (hit.document) layout.scroll.getOrElse(0) { 0f } else 0f
        val offsetY = if (hit.document) layout.scroll.getOrElse(1) { 0f } else 0f
        val rect = FrameRect(hit.rect[0] - offsetX, hit.rect[1] - offsetY, hit.rect[2], hit.rect[3])
        val merged = layout.copy(rects = layout.rects + (hit.key to rect))
        val bytes = Ids.bytes(spec.id) ?: return
        scene.setFrameLayout(bytes, FrameLayouts.coreJson(merged, spec.preset))
    }
}
