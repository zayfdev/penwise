package dev.penwise.frames

import android.content.Context
import android.os.SystemClock
import android.view.MotionEvent
import android.view.ViewConfiguration
import android.widget.FrameLayout
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.findViewTreeLifecycleOwner
import dev.penwise.ink.FitMode
import dev.penwise.ink.FrameBinder
import dev.penwise.ink.FrameHost
import dev.penwise.ink.PageRect
import dev.penwise.ink.PageScene
import dev.penwise.ink.PenActivity
import dev.penwise.ink.PendingBinding
import dev.penwise.ink.ViewTransform
import dev.penwise.ink.ViewportState

/**
 * The frames under the ink layer (design harness v2): the server's stills of every frame at its box on the board, and
 * on top of them at most one live WebView, the page of the focused frame, placed at the live window the core computes
 * from the page's actual scroll. The frame that was live before stays warm (hidden, paused) for a quick return; older
 * ones are destroyed. In the background the live page is paused and the stills stop polling.
 *
 * The WebViews are ordinary window views; the ink is a `SurfaceView` with `setZOrderOnTop`, above them. This view must
 * therefore not draw anything itself: the paper below is [dev.penwise.ink.PaperView]'s job, the stills [StillView]'s.
 *
 * The parts: [FrameLookup] (where the frames are), [LivePages] (the WebViews), [FrameStills], [FrameGestures] and
 * [FramePenHits] (touch and pen), [WindowFollower] (the scroll link) and [LayerInbox] (the bridge's messages). This
 * class wires them to the camera and decides who is live.
 */
internal class FrameLayer(
    context: Context,
) : FrameLayout(context),
    FrameHost,
    FrameBinder {
    private var specs: List<FrameSpec> = emptyList()
    private var viewport: ViewportState? = null
    private val rule = FocusRule()
    private val follow: (ViewTransform) -> Unit = {
        sync()
        if (!follower.linking) scheduleSettle()
    }

    /** The element keys each frame's bound strokes need; a `pw:watch` goes out when this changes. */
    private var watch: Map<String, List<String>> = emptyMap()
    private val settle = Runnable { settled() }
    private var active = true

    /** What the layer tells the screen around it (set by the stage). */
    val callbacks = LayerCallbacks()

    /** The pen, whose touch never carries the camera along with a page's scroll and ends a page's fling. */
    var pen: PenActivity? = null

    /** The scene's frame space, for a pen-down on a still (set from the UI thread). */
    var scene: PageScene? = null

    private val transform: () -> ViewTransform = { viewport?.transform ?: ViewTransform.IDENTITY }
    private val lookup = FrameLookup({ specs }, transform) { pages.live }
    private val stills =
        FrameStills(context, lookup, callbacks, { pages.live?.spec?.id == it }) { lookup.visibleIds(width, height) }
    private val follower =
        WindowFollower(
            camera = { viewport },
            screenHeight = { height },
            isLive = { it === pages.live },
            penActive = { pen?.active() == true },
            place = ::sync,
            clock = { SystemClock.uptimeMillis() },
        )
    private val touch = LayerTouch(lookup, follower, this, transform, ::onDoubleTap)
    private val gestures = FrameGestures(touch, ViewConfiguration.get(context).scaledTouchSlop.toFloat())
    private val penHits = FramePenHits(LayerPenHost(lookup, stills, { scene }))
    private val pages: LivePages =
        LivePages(this, context, ::deliver, { entry, x, y -> follower.onScroll(entry, x, y) }) { gestures.release(it) }
    private val inbox = LayerInbox(pages, stills, penHits, follower, this, { watch }, callbacks)
    private val lifecycle =
        LifecycleEventObserver { _, event ->
            when (event) {
                Lifecycle.Event.ON_STOP -> setActive(false)
                Lifecycle.Event.ON_START -> setActive(true)
                else -> Unit
            }
        }

    init {
        addView(stills.view, LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.MATCH_PARENT))
    }

    /** The PC server the stills and pen-down hits of non-live frames come from; null while none is known. */
    var server: FrameServer?
        get() = stills.server
        set(value) {
            stills.server = value
        }

    /** The live frame's id, or null. */
    val focusedFrame: String? get() = pages.live?.spec?.id

    override fun onAttachedToWindow() {
        super.onAttachedToWindow()
        findViewTreeLifecycleOwner()?.lifecycle?.addObserver(lifecycle)
    }

    override fun onDetachedFromWindow() {
        findViewTreeLifecycleOwner()?.lifecycle?.removeObserver(lifecycle)
        super.onDetachedFromWindow()
    }

    /** Places the frames through [viewport]; from the UI thread. */
    fun bind(viewport: ViewportState) {
        if (this.viewport === viewport) return
        this.viewport?.observers?.remove(follow)
        this.viewport = viewport
        viewport.observers.add(follow)
        stills.start()
        sync()
    }

    /**
     * The app is in front or not: in the background the live page is paused and the stills stop polling; both pick
     * up again when it comes back.
     */
    fun setActive(active: Boolean) {
        if (this.active == active) return
        this.active = active
        pages.setRunning(active)
        if (active) stills.resume() else stills.pause()
    }

    /** The frame a board that just got its frames still has to open on ([FrameOpening]); null once done. */
    private var opening: String? = null

    /**
     * Shows [next]: a frame that is gone is closed (and its page destroyed when it was live or warm), a live or warm
     * frame whose URL changed is loaded again, a resized one is re-laid out. A frame that stays keeps its WebView and
     * its page state. Pages open when a frame gets the focus, never here.
     */
    fun setFrames(next: List<FrameSpec>) {
        val plan = FrameDiff.plan(specs, next)
        val first = specs.isEmpty()
        specs = next
        for (id in plan.close) closeFrame(id)
        pages.update(plan, next)
        sync()
        for (entry in pages.all()) PageOps.sendPatch(entry)
        scheduleSettle()
        stills.refresh()
        if (first) opening = FrameOpening.target(next)
    }

    /** The Ghost chip's on/off toggle: a local before/after view of the frame's patch, never synced. */
    fun setPatchOn(
        id: String,
        on: Boolean,
    ) {
        val entry = pages.entry(id) ?: return
        if (entry.patchOn == on) return
        entry.patchOn = on
        PageOps.sendPatch(entry)
    }

    /**
     * The element keys each frame's bound strokes need (`sceneFrameKeys`). Every page gets the new list; the bridge
     * replies with a full layout message, so a key that changed its rect is re-read at once.
     */
    fun setWatch(keys: Map<String, List<String>>) {
        if (keys == watch) return
        watch = keys
        for (entry in pages.all()) PageOps.sendWatch(entry, watch[entry.spec.id].orEmpty())
    }

    /** Fits frame [id] to the screen (animated) and makes it the live one: a double-tap, an agent's `Focus`. */
    fun fitFrame(
        id: String,
        mode: FitMode = FitMode.FIT_WIDTH,
    ) {
        val spec = specs.firstOrNull { it.id == id } ?: return
        viewport?.fitRect(PageRect(spec.x, spec.y, spec.cssWidth.toFloat(), spec.boxHeight.toFloat()), mode)
        if (mode == FitMode.FIT_WIDTH) {
            rule.explicit(id)
            applyFocus(id)
        }
    }

    /** The core's live window for frame [id], computed for the scroll [scrollX], [scrollY] the bridge had reported. */
    fun setWindow(
        id: String,
        window: FloatArray,
        scrollX: Float,
        scrollY: Float,
    ) {
        val entry = pages.entry(id) ?: return
        // Kept relative to the frame, so a frame that moves takes its window along.
        entry.base = WindowBase(window[0] - entry.spec.x, window[1] - entry.spec.y, scrollX, scrollY, entry.spec.full)
        sync()
    }

    /** Loads a frame's page again (the chip's reload), or asks the server for a fresh still of a frame not live. */
    fun reload(id: String) {
        val entry = pages.entry(id)
        if (entry == null) stills.refresh(force = true) else PageOps.reload(entry)
    }

    /** Replays [state]'s recipe on [frameId]'s page (a mark chip's "Replay state"). */
    fun replay(
        frameId: String,
        state: String,
    ) {
        pages.entry(frameId)?.let { FrameCommands.replay(it.view, state) }
    }

    /** Destroys the WebViews and stops the stills (page leave); a frame that comes back is opened again. */
    fun release() {
        removeCallbacks(settle)
        viewport?.observers?.remove(follow)
        viewport = null
        gestures.reset()
        penHits.clear()
        stills.release()
        rule.clear()
        pages.destroyAll()
    }

    override fun route(
        event: MotionEvent,
        handOver: (MotionEvent) -> Unit,
    ): Boolean = gestures.route(event, handOver)

    override fun cancel() {
        // Called for every stylus event: nothing to do unless a finger gesture, a waiting tap or a fling is about.
        if (!gestures.busy && !follower.engaged) return
        gestures.cancel()
        // The pen is down: a fling of the page ends with it.
        follower.penDown(pages.live)
    }

    override fun beginBinding(
        windowX: Float,
        windowY: Float,
        bindToElement: Boolean,
    ): PendingBinding? = penHits.begin(windowX, windowY, bindToElement)

    /** A one-finger double-tap on [frameId]'s box: it is fitted and made live, and other devices may follow. */
    private fun onDoubleTap(frameId: String) {
        val live = pages.live
        // A live page that already fills the screen gets its own double-tap.
        if (live?.spec?.id == frameId && touch.isFitted(live)) return
        fitFrame(frameId)
        callbacks.onUserFocus?.invoke(frameId)
    }

    /** Makes [id] the live frame (null: none): the previous live page is parked warm, the one before it destroyed. */
    private fun applyFocus(id: String?) {
        val was = pages.live?.spec?.id
        val left = pages.focus(id?.let(lookup::spec))
        val now = pages.live?.spec?.id
        if (now == was) return
        left?.let { stills.demoted(it.spec.id) }
        sync()
        callbacks.onFocused?.invoke(now)
    }

    private fun closeFrame(id: String) {
        stills.forget(id)
        if (rule.focused == id) rule.clear()
        val wasLive = pages.live?.spec?.id == id
        pages.close(id)
        if (wasLive) callbacks.onFocused?.invoke(null)
    }

    /** Moves the live page to its window on screen: its page keeps the preset layout size, only its position scales. */
    private fun sync() {
        val view = transform()
        stills.show(view)
        val entry = pages.live ?: return
        val placement = FrameGeometry.place(entry.window(), view, entry.pxPerCss)
        entry.placement = placement
        entry.view.translationX = placement.left
        entry.view.translationY = placement.top
        entry.view.scaleX = placement.scale
        entry.view.scaleY = placement.scale
        // A variant the owner did not pick is dimmed until (and unless) it wins: the comparison stays on screen.
        entry.view.alpha = if (isDimmed(entry.spec)) DIMMED_ALPHA else 1f
    }

    private fun scheduleSettle() {
        removeCallbacks(settle)
        postDelayed(settle, SETTLE_MS)
    }

    /** The camera has been still for a moment: the focus rule decides who is live, the window covers what is seen. */
    private fun settled() {
        val camera = viewport ?: return
        if (camera.gesturing || camera.animating) {
            scheduleSettle()
            return
        }
        // A board that just got its frames opens fitted on one, once the camera knows the screen and the page.
        opening?.let { id ->
            if (!camera.hasSize) {
                scheduleSettle()
                return
            }
            opening = null
            fitFrame(id)
            return
        }
        if (width > 0 && height > 0) {
            val boxes = specs.map { lookup.boxOnScreen(it, camera.transform) }
            applyFocus(rule.evaluate(boxes, width.toFloat(), height.toFloat()))
        }
        pages.live?.let(follower::align)
        stills.refresh()
    }

    /** A bridge message, on the UI thread: the page's scroll, layout and route reports; hits and replies. */
    private fun deliver(
        frameId: String,
        message: FrameMessage,
    ) {
        // The bridge posts on the WebView's thread; views and state belong to the UI thread.
        post {
            val entry = pages.entry(frameId) ?: return@post
            FrameInbox.handle(entry, message, inbox)
        }
    }

    private val density: Float get() = resources.displayMetrics.density

    private companion object {
        /** The camera must rest this long before the focus rule looks at it again. */
        const val SETTLE_MS = 150L

        /** A variant the owner did not pick: dimmed, so the picked one stands out at a glance. */
        const val DIMMED_ALPHA = 0.6f
    }
}
