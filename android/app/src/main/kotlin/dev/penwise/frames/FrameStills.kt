package dev.penwise.frames

import android.content.Context
import dev.penwise.ink.HitAnswer
import dev.penwise.ink.ViewTransform

/**
 * The stills of the layer's frames: the polling feed, the view that draws them, and the layout each frame last had
 * (the live page's bridge report, or the still's while the frame is not live). While a frame is not live its still's
 * layout goes to the core, so the bound marks sit right on the picture.
 */
internal class FrameStills(
    context: Context,
    private val lookup: FrameLookup,
    private val callbacks: LayerCallbacks,
    private val isLive: (String) -> Boolean,
    visible: () -> List<String>,
) {
    private val feed = StillFeed(context, ::onManifest) { show(transform) }
    private var transform = ViewTransform.IDENTITY
    private val sources = visible

    /** The view that draws the stills. */
    val view = StillView(context, feed.bitmaps)

    /** The layout each frame last had: the live page's bridge report, or its still's while it is not live. */
    val layouts = HashMap<String, FrameMessage.Layout>()

    /** The PC server the stills and pen-down hits come from; null while none is known. */
    var server: FrameServer?
        get() = feed.server
        set(value) {
            if (feed.server == value) return
            feed.server = value
            feed.refresh(force = true)
        }

    /** Draws the stills under the camera [view]. */
    fun show(view: ViewTransform) {
        transform = view
        this.view.show(lookup.specs(), feed.manifests, view)
    }

    fun start() = feed.start(sources)

    /** The app is in the background: no polling; what is drawn stays. */
    fun pause() = feed.stop()

    fun resume() = feed.resume()

    fun refresh(force: Boolean = false) = feed.refresh(force)

    /** [frameId] stopped being live: the core takes the still's layout again, so bound marks sit right on it. */
    fun demoted(frameId: String) {
        feed.manifests[frameId]?.let { publish(frameId, it, LayoutKind.DEMOTED) }
    }

    fun forget(frameId: String) {
        feed.forget(frameId)
        layouts.remove(frameId)
    }

    fun hit(
        frameId: String,
        x: Float,
        y: Float,
        done: (HitAnswer?) -> Unit,
    ) = feed.hit(frameId, x, y, done)

    fun release() = feed.release()

    /** A new still: drawn at once; while the frame is not live its layout goes to the core. */
    private fun onManifest(
        frameId: String,
        manifest: StillManifest,
    ) {
        show(transform)
        if (!isLive(frameId)) publish(frameId, manifest, LayoutKind.STILL)
    }

    private fun publish(
        frameId: String,
        manifest: StillManifest,
        kind: LayoutKind,
    ) {
        val spec = lookup.spec(frameId) ?: return
        val layout = manifest.asLayout()
        layouts[frameId] = layout
        callbacks.onLayout?.invoke(frameId, FrameLayouts.coreJson(layout, spec.preset), kind, layout.state)
    }
}
