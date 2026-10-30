package dev.penwise.ink

import org.json.JSONObject

/** How a stroke drawn on a frame follows the page: the element it was drawn on, or a document position. */
enum class BindMode(
    val wire: String,
) {
    BOUND("bound"),
    AREA("area"),
}

/** The answer to a hit request: the stable element key and its rect in the page's CSS viewport px (the bridge) or
 * document px (the server). */
internal data class HitAnswer(
    val key: String,
    val rect: List<Float>,
    /** The server answered for a point off the live window: the rect is in document px, not viewport px. */
    val document: Boolean = false,
)

/**
 * Which binding a stroke gets (K3): a pen-down on a frame is bound to its element when one answered the hit request,
 * otherwise it is pinned to the area; a pen-down off every frame has no binding at all.
 */
internal object BindDecision {
    fun mode(
        onFrame: Boolean,
        bindToElement: Boolean,
        key: String?,
    ): BindMode? =
        when {
            !onFrame -> null
            bindToElement && !key.isNullOrEmpty() -> BindMode.BOUND
            else -> BindMode.AREA
        }
}

/**
 * A stroke's frame binding, decided at pen-up. The element key arrives from the bridge while the stroke is drawn.
 * A bound stroke is only possible once the core has the element's rect, so [prepare] merges the hit's rect into the
 * frame's layout just before the commit (the core's intended first-draw flow); an unanswered request, or the "pin to
 * area" mode, commits an area binding.
 */
internal class PendingBinding(
    val frame: String,
    private val bindToElement: Boolean,
    private val answer: () -> HitAnswer?,
    private val prime: (scene: PageScene, hit: HitAnswer) -> Unit = { _, _ -> },
    private val onDone: () -> Unit = {},
) {
    /** Asked once: the bridge answer must not change between the decision and the commit. */
    private val hit: HitAnswer? by lazy { answer() }

    private val key: String? get() = hit?.key?.takeIf { it.isNotEmpty() }

    /** Merges the hit rect into the frame's layout so a bound stroke drawn on a new element resolves. */
    fun prepare(scene: PageScene?) {
        val answer = hit ?: return
        if (scene == null || key == null || answer.rect.size < RECT_VALUES) return
        prime(scene, answer)
    }

    /** The `sceneAddBoundStroke` binding JSON, or null when there is no binding (never for a frame). */
    fun json(): String? {
        val mode = BindDecision.mode(onFrame = true, bindToElement = bindToElement, key = key) ?: return null
        val message = JSONObject().put("frame", frame).put("mode", mode.wire)
        if (mode == BindMode.BOUND) message.put("key", key)
        return message.toString()
    }

    /** Forgets the pending hit request (called once the binding is committed or abandoned). */
    fun release() = onDone()

    private companion object {
        const val RECT_VALUES = 4
    }
}

/** What the pen asks the frames at pen-down; the holder lets the surface keep one across page and frame changes. */
internal interface FrameBinder {
    /** The frame under a screen point, with a hit request on its way; null when the pen is off every frame. */
    fun beginBinding(
        windowX: Float,
        windowY: Float,
        bindToElement: Boolean,
    ): PendingBinding?
}

/** The binder the surface reads on the UI thread, swapped whenever the frames come or go. */
internal class FrameBinderHolder : FrameBinder {
    @Volatile
    var delegate: FrameBinder? = null

    override fun beginBinding(
        windowX: Float,
        windowY: Float,
        bindToElement: Boolean,
    ): PendingBinding? = delegate?.beginBinding(windowX, windowY, bindToElement)
}
