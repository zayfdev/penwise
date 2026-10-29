package dev.penwise.frames

import android.content.Context
import android.os.SystemClock
import android.util.Log
import android.view.InputDevice
import android.view.MotionEvent
import android.widget.FrameLayout
import dev.penwise.ink.FrameBinder
import dev.penwise.ink.FrameHost
import dev.penwise.ink.HitAnswer
import dev.penwise.ink.PageScene
import dev.penwise.ink.PendingBinding
import dev.penwise.ink.ViewTransform
import dev.penwise.ink.ViewportState
import dev.penwise.sync.Ids

/**
 * The live pages under the ink layer (§6.4): one WebView per [FrameSpec], laid out at its preset size and moved by the
 * canvas [ViewportState] (position and scale, so panning and pinching carry the pages along), plus the routing that
 * keeps one finger for the page and everything else for the canvas.
 *
 * The WebViews are ordinary window views; the ink is a `SurfaceView` with `setZOrderOnTop`, above them. This view must
 * therefore not draw anything: the paper below the pages is [dev.penwise.ink.PaperView]'s job.
 *
 * Input never reaches the WebViews through the view tree (the ink surface is above and takes every touch): a frame
 * gesture synthesises single-pointer events in the WebView's own coordinates and dispatches them, which is what keeps
 * scrolling and clicking native.
 */
internal class FrameLayer(
    context: Context,
) : FrameLayout(context),
    FrameHost,
    FrameBinder {
    private val entries = LinkedHashMap<String, FrameEntry>()
    private val follow: (ViewTransform) -> Unit = { sync() }
    private var viewport: ViewportState? = null

    /** The element keys each frame's bound strokes need; a `pw:watch` goes out when this changes. */
    private var watch: Map<String, List<String>> = emptyMap()
    private val hits = FrameHits()

    private var mode = Mode.IDLE
    private var active: FrameEntry? = null
    private var activePointer = MotionEvent.INVALID_POINTER_ID
    private var downTime = 0L
    private var localX = 0f
    private var localY = 0f

    /** Called on the UI thread whenever a page reports a new scroll offset (the ink layer reads it for area marks). */
    var onScroll: ((String, FrameScroll) -> Unit)? = null

    /**
     * Called on the UI thread when the user's own navigation inside a page (a link click) lands on another route, at
     * most once per settled navigation: the frame item's `route` follows, without a reload.
     */
    var onRoute: ((String, String) -> Unit)? = null

    /**
     * Called on the UI thread for every `pw:layout`: the frame id, the layout as the core takes it (its preset is
     * filled here) and the page's state recipe ("" = the initial UI).
     */
    var onLayout: ((frameId: String, json: String, state: String) -> Unit)? = null

    /** Places the frames through [viewport]; from the UI thread. */
    fun bind(viewport: ViewportState) {
        if (this.viewport === viewport) return
        this.viewport?.observers?.remove(follow)
        this.viewport = viewport
        viewport.observers.add(follow)
        sync()
    }

    /**
     * Shows [specs]: a new frame's WebView is opened, a frame whose URL changed is loaded again (or adopted when the
     * page already shows that route), a resized frame is re-laid out, a moved one is translated, and a frame that is
     * gone is destroyed. A frame that stays keeps its WebView and its page state (its scroll position included).
     *
     * A patch, label, group or choice change is none of those: every spec is taken, then the patch decisions go out
     * and [sync] re-applies the variant dimming.
     */
    fun setFrames(specs: List<FrameSpec>) {
        val plan = FrameDiff.plan(entries.values.map { it.spec }, specs)
        if (!plan.isEmpty) {
            for (id in plan.close) entries.remove(id)?.let { release(it) }
            for (spec in plan.open) {
                val view = FrameViews.open(context, spec, density) { message -> deliver(spec.id, message) }
                entries[spec.id] = FrameEntry(spec, view)
                addView(view)
            }
            for (spec in plan.load) entries[spec.id]?.let { load(it, spec) }
            for (spec in plan.resize) entries[spec.id]?.let { resize(it, spec) }
        }
        for (spec in specs) entries[spec.id]?.let { it.spec = spec }
        sync()
        for (entry in entries.values) sendPatch(entry)
    }

    /** The Ghost chip's on/off toggle: a local before/after view of the frame's patch, never synced. */
    fun setPatchOn(
        id: String,
        on: Boolean,
    ) {
        val entry = entries[id] ?: return
        if (entry.patchOn == on) return
        entry.patchOn = on
        sendPatch(entry)
    }

    /**
     * The element keys each frame's bound strokes need (`sceneFrameKeys`). Every frame gets the new list; the bridge
     * replies with a full layout message, so a key that changed its rect is re-read at once.
     */
    fun setWatch(keys: Map<String, List<String>>) {
        if (keys == watch) return
        watch = keys
        for (entry in entries.values) sendWatch(entry)
    }

    /**
     * A pen-down at ([windowX], [windowY]): the frame under it (null off every frame) and a `pw:hit?` on its way to
     * the page. The answer is read when the stroke commits; no answer by then means an area binding.
     */
    override fun beginBinding(
        windowX: Float,
        windowY: Float,
        bindToElement: Boolean,
    ): PendingBinding? {
        val index = FrameGeometry.at(windowX, windowY, placements())
        if (index < 0) return null
        val entry = entries.values.elementAt(index)
        val x = entry.placement.toCssX(windowX, density)
        val y = entry.placement.toCssY(windowY, density)
        val id = hits.ask(entry.view, x, y)
        return PendingBinding(
            frame = entry.spec.id,
            bindToElement = bindToElement,
            answer = { hits.take(id) },
            prime = { scene, hit -> primeLayout(entry, scene, hit) },
            onDone = { hits.forget(id) },
        )
    }

    /**
     * The hit's element may be one the page never watched, so the core lacks its rect. Merge the rect the bridge just
     * measured into the frame's last layout before the stroke commits (M2Core's first-draw flow).
     */
    private fun primeLayout(
        entry: FrameEntry,
        scene: PageScene,
        hit: HitAnswer,
    ) {
        val layout = entry.layout ?: return
        val rect = FrameRect(hit.rect[0], hit.rect[1], hit.rect[2], hit.rect[3])
        val merged = layout.copy(rects = layout.rects + (hit.key to rect))
        val bytes = Ids.bytes(entry.spec.id) ?: return
        scene.setFrameLayout(bytes, FrameLayouts.coreJson(merged, entry.spec.preset))
    }

    /** Replays [state]'s recipe on [frameId]'s page (a mark chip's "Replay state"). */
    fun replay(
        frameId: String,
        state: String,
    ) {
        entries[frameId]?.let { FrameCommands.replay(it.view, state) }
    }

    private fun sendWatch(entry: FrameEntry) {
        FrameCommands.watch(entry.view, watch[entry.spec.id].orEmpty())
    }

    /** Loads one frame's page again (the chip's reload); its scroll position comes back through the bridge. */
    fun reload(id: String) {
        val entry = entries[id] ?: return
        entry.reportedRoute = entry.spec.route
        entry.quietUntil = SystemClock.uptimeMillis() + LOAD_QUIET_MS
        entry.view.reload()
    }

    /** Destroys the WebViews (page leave); opening a frame again creates a fresh one. */
    fun release() {
        viewport?.observers?.remove(follow)
        viewport = null
        cancelFrame()
        hits.clear()
        mode = Mode.IDLE
        for (entry in entries.values) release(entry)
        entries.clear()
    }

    /** Takes one page's WebView down; its content is gone, so a frame that comes back loads again. */
    private fun release(entry: FrameEntry) {
        // A frame deleted under the finger: the page gets a cancel, never a click into a destroyed view.
        if (active === entry) cancelFrame()
        entry.routeRunnable?.let(::removeCallbacks)
        entry.routeRunnable = null
        entry.pendingRoute = null
        removeView(entry.view)
        entry.view.destroy()
    }

    override fun route(
        event: MotionEvent,
        handOver: (MotionEvent) -> Unit,
    ): Boolean =
        when (mode) {
            Mode.CANVAS -> {
                // The canvas owns this sequence until every finger is up; a lone remaining finger must not click.
                if (event.actionMasked == MotionEvent.ACTION_UP || event.actionMasked == MotionEvent.ACTION_CANCEL) {
                    mode = Mode.IDLE
                }
                false
            }

            Mode.FRAME -> {
                continueFrame(event, handOver)
            }

            Mode.IDLE -> {
                begin(event)
            }
        }

    override fun cancel() {
        cancelFrame()
        if (mode == Mode.FRAME) mode = Mode.IDLE
    }

    /** One finger went down: it belongs to the page under it, or to nobody when it is outside every page. */
    private fun begin(event: MotionEvent): Boolean {
        if (event.actionMasked != MotionEvent.ACTION_DOWN || fingers(event) != 1) return false
        val index = FrameGeometry.at(event.getX(0), event.getY(0), placements())
        if (index < 0) return false
        active = entries.values.elementAt(index)
        activePointer = event.getPointerId(0)
        downTime = event.downTime
        forward(event, MotionEvent.ACTION_DOWN)
        mode = Mode.FRAME
        return true
    }

    private fun continueFrame(
        event: MotionEvent,
        handOver: (MotionEvent) -> Unit,
    ): Boolean {
        if (!FrameTouch.keepsFrame(fingers(event))) {
            // A second finger takes the gesture: the page gets a cancel (no stray click), the canvas every finger down.
            cancelFrame()
            mode = Mode.CANVAS
            handOver(event)
            return false
        }
        if (event.actionMasked == MotionEvent.ACTION_CANCEL) {
            cancelFrame()
            mode = Mode.IDLE
            return true
        }
        val action = if (event.actionMasked == MotionEvent.ACTION_UP) MotionEvent.ACTION_UP else MotionEvent.ACTION_MOVE
        forward(event, action)
        if (event.actionMasked == MotionEvent.ACTION_UP) {
            active = null
            mode = Mode.IDLE
        }
        return true
    }

    /** Moves the page rect on screen: the page keeps its layout size, so its CSS breakpoints see the preset. */
    private fun sync() {
        val view = viewport?.transform ?: ViewTransform.IDENTITY
        for (entry in entries.values) {
            val placement = FrameGeometry.place(rect(entry.spec), view, density)
            entry.placement = placement
            entry.view.translationX = placement.left
            entry.view.translationY = placement.top
            entry.view.scaleX = placement.scale
            entry.view.scaleY = placement.scale
            // A variant the owner did not pick is dimmed until (and unless) it wins: the comparison stays on screen.
            entry.view.alpha = if (isDimmed(entry.spec)) DIMMED_ALPHA else 1f
        }
    }

    /** Sends the patch decision the page does not have yet; a silent no-op when it already shows that. */
    private fun sendPatch(entry: FrameEntry) {
        val message = entry.patches.next(entry.spec.patch, entry.patchOn) ?: return
        FrameCommands.patch(entry.view, message)
    }

    /** The item's page box (C1): `css_w × css_h` page units, whatever the display density. */
    private fun rect(spec: FrameSpec): FrameRect =
        FrameRect(spec.x, spec.y, spec.cssWidth.toFloat(), spec.cssHeight.toFloat())

    private fun placements(): List<FramePlacement> = entries.values.map { it.placement }

    /** A finger event in the active page's own coordinates; the WebView sees a normal single-pointer sequence. */
    private fun forward(
        event: MotionEvent,
        action: Int,
    ) {
        val entry = active ?: return
        val index = event.findPointerIndex(activePointer)
        if (index < 0) return
        localX = entry.placement.toLocalX(event.getX(index))
        localY = entry.placement.toLocalY(event.getY(index))
        dispatch(entry, action, event.eventTime)
    }

    private fun dispatch(
        entry: FrameEntry,
        action: Int,
        time: Long,
    ) {
        val copy = MotionEvent.obtain(downTime, time, action, localX, localY, 0)
        // A touchscreen finger, not SOURCE_UNKNOWN: the WebView reads the tool type to decide it is a finger, and the
        // page's pointer events should say so too.
        copy.source = InputDevice.SOURCE_TOUCHSCREEN
        entry.view.dispatchTouchEvent(copy)
        copy.recycle()
    }

    private fun cancelFrame() {
        val entry = active ?: return
        // The coordinates of the last event, at the time the gesture is abandoned.
        dispatch(entry, MotionEvent.ACTION_CANCEL, SystemClock.uptimeMillis())
        active = null
    }

    private enum class Mode {
        /** Nothing of ours is down. */
        IDLE,

        /** One finger is working a page. */
        FRAME,

        /** A gesture that began on a page became a canvas gesture; the canvas owns it until every finger is up. */
        CANVAS,
    }

    /**
     * Points [entry] at [spec]: a page that already shows the spec's route (the user clicked a link; the item follows)
     * adopts it without a reload, anything else (another project, another host, a route typed by the user) loads.
     */
    private fun load(
        entry: FrameEntry,
        spec: FrameSpec,
    ) {
        val adopted =
            entry.reportedRoute == spec.route && FrameUrls.origin(entry.loadedUrl) == FrameUrls.origin(spec.url)
        entry.spec = spec
        if (adopted) {
            entry.loadedUrl = spec.url
            return
        }
        entry.loadedUrl = spec.url
        entry.reportedRoute = spec.route
        entry.quietUntil = SystemClock.uptimeMillis() + LOAD_QUIET_MS
        entry.view.loadUrl(spec.url)
    }

    /** A preset switch: the page keeps its document and reflows at the new width (its breakpoints see the preset). */
    private fun resize(
        entry: FrameEntry,
        spec: FrameSpec,
    ) {
        entry.spec = spec
        entry.view.layoutParams = LayoutParams((spec.cssWidth * density).toInt(), (spec.cssHeight * density).toInt())
    }

    /**
     * A bridge message: logged, and the scroll offset kept on the entry for the ink layer (M2's area marks). A page's
     * own navigation updates its item's route, once the clicking stops.
     */
    private fun deliver(
        frameId: String,
        message: FrameMessage,
    ) {
        // The bridge posts on the WebView's thread; views and state belong to the UI thread.
        post {
            val entry = entries[frameId] ?: return@post
            when (message) {
                is FrameMessage.Hello -> {
                    // The document size is known before the first scroll: the readout and M2's area marks want it now.
                    entry.scroll = FrameScroll(entry.scroll.x, entry.scroll.y, message.docWidth, message.docHeight)
                    FrameRoutes.ofUrl(message.url)?.let { entry.reportedRoute = it }
                    onScroll?.invoke(frameId, entry.scroll)
                    // A fresh document starts with no watched keys: ask for the marks' ones before the first layout.
                    sendWatch(entry)
                    // A fresh document also starts without the ghost/variant patch: apply it again at once.
                    entry.patches.reset()
                    sendPatch(entry)
                    Log.i(
                        TAG,
                        "$frameId hello ${message.url} doc ${message.docWidth}x${message.docHeight} " +
                            "view ${message.viewWidth}x${message.viewHeight}",
                    )
                }

                is FrameMessage.Scroll -> {
                    entry.scroll = FrameScroll(message.x, message.y, message.docWidth, message.docHeight)
                    Log.d(TAG, "$frameId scroll ${message.x},${message.y} of ${message.docWidth}x${message.docHeight}")
                    onScroll?.invoke(frameId, entry.scroll)
                }

                is FrameMessage.Route -> {
                    queueRoute(entry, message.route)
                }

                is FrameMessage.Hit -> {
                    hits.answer(message.id, message.key, message.rect)
                    Log.d(TAG, "$frameId hit ${message.key} '${message.selector}' ${message.rect}")
                }

                is FrameMessage.Layout -> {
                    entry.layout = message
                    onLayout?.invoke(frameId, FrameLayouts.coreJson(message, entry.spec.preset), message.state)
                }

                is FrameMessage.Replayed -> {
                    Log.i(TAG, "$frameId replay '${message.state}' ok=${message.ok}")
                }

                is FrameMessage.Patched -> {
                    Log.i(TAG, "$frameId patched ok=${message.ok} failed=${message.failed}")
                }
            }
        }
    }

    /**
     * The page navigated by itself. Its item follows when the user stops: a menu that pushes several routes while it
     * settles must not queue an op each, and the page is already there, so the item update reloads nothing.
     */
    private fun queueRoute(
        entry: FrameEntry,
        route: String,
    ) {
        val normalized = FrameRoutes.normalize(route) ?: return
        // Already known (a repeated report of the same navigation) or ignored: nothing to tell the view model.
        if (SystemClock.uptimeMillis() < entry.quietUntil) return
        if (normalized == entry.reportedRoute || normalized == entry.spec.route) return
        entry.reportedRoute = normalized
        entry.pendingRoute = normalized
        val runnable = entry.routeRunnable ?: Runnable { commitRoute(entry) }.also { entry.routeRunnable = it }
        removeCallbacks(runnable)
        postDelayed(runnable, ROUTE_DEBOUNCE_MS)
    }

    private fun commitRoute(entry: FrameEntry) {
        val route = entry.pendingRoute ?: return
        entry.pendingRoute = null
        onRoute?.invoke(entry.spec.id, route)
    }

    /** The page's pointer types in this event: palms and canceled pointers never count. */
    private fun fingers(event: MotionEvent): Int {
        var count = 0
        if (event.flags and MotionEvent.FLAG_CANCELED != 0) return 0
        for (i in 0 until event.pointerCount) {
            if (event.getToolType(i) == MotionEvent.TOOL_TYPE_FINGER) count++
        }
        return count
    }

    private val density: Float get() = resources.displayMetrics.density

    private companion object {
        const val TAG = "PenwiseFrames"

        /** How long a route report is held so a flurry of link clicks becomes one item update. */
        const val ROUTE_DEBOUNCE_MS = 500L

        /** Route reports right after a programmatic load describe the document being replaced. */
        const val LOAD_QUIET_MS = 1_500L

        /** A variant the owner did not pick: dimmed, so the picked one stands out at a glance. */
        const val DIMMED_ALPHA = 0.6f
    }
}
