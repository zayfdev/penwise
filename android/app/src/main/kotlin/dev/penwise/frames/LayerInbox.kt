package dev.penwise.frames

import android.view.View

/** The layer's side of the bridge messages: what each page's report changes in the layer and the screen around it. */
internal class LayerInbox(
    private val pages: LivePages,
    private val stills: FrameStills,
    private val penHits: FramePenHits,
    private val follower: WindowFollower,
    private val view: View,
    private val watch: () -> Map<String, List<String>>,
    private val callbacks: LayerCallbacks,
) : InboxHost {
    override fun isLive(entry: FrameEntry): Boolean = entry === pages.live

    override fun scrolled(
        entry: FrameEntry,
        scroll: FrameScroll,
    ) {
        callbacks.onScroll?.invoke(entry.spec.id, scroll)
    }

    override fun layout(
        entry: FrameEntry,
        message: FrameMessage.Layout,
        json: String,
    ) {
        stills.layouts[entry.spec.id] = message
        callbacks.onLayout?.invoke(entry.spec.id, json, LayoutKind.LIVE, message.state)
        if (entry.needsAlign) follower.align(entry)
    }

    override fun hello(entry: FrameEntry) {
        // A fresh document starts with no watched keys: ask for the marks' ones before the first layout.
        PageOps.sendWatch(entry, watch()[entry.spec.id].orEmpty())
        entry.patches.reset()
        PageOps.sendPatch(entry)
        if (entry.needsAlign) follower.align(entry)
    }

    override fun answered(
        id: Int,
        key: String,
        rect: List<Float>,
    ) = penHits.answered(id, key, rect)

    override fun reschedule(
        commit: Runnable,
        delayMs: Long,
    ) {
        view.removeCallbacks(commit)
        view.postDelayed(commit, delayMs)
    }

    override fun committed(
        entry: FrameEntry,
        route: String,
    ) {
        callbacks.onRoute?.invoke(entry.spec.id, route)
    }
}
