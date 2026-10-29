package dev.penwise.frames

import org.json.JSONObject

/**
 * What the tablet sends into a page for its ghost/variant patch (P3): `{t:"pw:patch", patch, on}` applies the patch
 * (`on=true`) or reverts to the pre-patch DOM (`on=false`). The bridge forgets the patch on every load, so [reset]
 * after a `pw:hello` makes the next [next] re-send it.
 *
 * Pure and unit-tested: the decision "send, or nothing" is the interesting part, not the WebView call.
 */
internal class PatchFeed {
    private var sent = false
    private var sentPatch: String? = null
    private var sentOn = false

    /** A fresh document: nothing of ours is applied, so the next [next] always sends. */
    fun reset() {
        sent = false
        sentPatch = null
        sentOn = false
    }

    /**
     * The message to send for [patch] (the P1 JSON text, or null for a frame without one) and the local toggle [on],
     * or null when the page already shows that. A patch that is removed is reverted, when one was applied.
     */
    fun next(
        patch: String?,
        on: Boolean,
    ): JSONObject? {
        val usable = patch?.takeIf(::parses) ?: return revert()
        // A fresh document is already unpatched: nothing to revert when the owner keeps the Ghost off.
        if (!sent && !on) return null
        if (sent && sentPatch == usable && sentOn == on) return null
        record(sentPatch = usable, on = on)
        return message(usable, on)
    }

    /** Whatever was applied goes away, once; nothing to do on a page that never got a patch. */
    private fun revert(): JSONObject? {
        if (!sent || sentPatch == null) return null
        record(sentPatch = null, on = false)
        return message(null, on = false)
    }

    private fun record(
        sentPatch: String?,
        on: Boolean,
    ) {
        sent = true
        this.sentPatch = sentPatch
        sentOn = on
    }

    private fun message(
        patch: String?,
        on: Boolean,
    ): JSONObject =
        JSONObject()
            .put("t", "pw:patch")
            .put("patch", patch?.let(::JSONObject) ?: JSONObject.NULL)
            .put("on", on)

    /** A patch the server accepted (P1) is a JSON object; anything else is not applied and never becomes a message. */
    private fun parses(patch: String): Boolean = runCatching { JSONObject(patch) }.isSuccess
}
