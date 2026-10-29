package dev.penwise.frames

import android.annotation.SuppressLint
import android.content.Context
import android.graphics.Color
import android.os.SystemClock
import android.util.Log
import android.view.InputDevice
import android.view.MotionEvent
import android.webkit.WebSettings
import android.webkit.WebView
import android.webkit.WebViewClient
import android.widget.FrameLayout
import dev.penwise.BuildConfig
import dev.penwise.ink.FrameHost
import dev.penwise.ink.ViewTransform
import dev.penwise.ink.ViewportState

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
    FrameHost {
    private val entries = LinkedHashMap<String, Entry>()
    private val follow: (ViewTransform) -> Unit = { sync() }
    private var viewport: ViewportState? = null

    private var mode = Mode.IDLE
    private var active: Entry? = null
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
     */
    fun setFrames(specs: List<FrameSpec>) {
        val plan = FrameDiff.plan(entries.values.map { it.spec }, specs)
        if (plan.isEmpty) return
        for (id in plan.close) entries.remove(id)?.let { release(it) }
        for (spec in plan.open) {
            val view = open(spec)
            entries[spec.id] = Entry(spec, view)
            addView(view)
        }
        for (spec in plan.load) entries[spec.id]?.let { load(it, spec) }
        for (spec in plan.resize) entries[spec.id]?.let { resize(it, spec) }
        for (spec in plan.move) entries[spec.id]?.let { it.spec = spec }
        sync()
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
        mode = Mode.IDLE
        for (entry in entries.values) release(entry)
        entries.clear()
    }

    /** Takes one page's WebView down; its content is gone, so a frame that comes back loads again. */
    private fun release(entry: Entry) {
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
        }
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
        entry: Entry,
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

    /**
     * One frame's page. A frame's window view: a WebView at the preset's device size, fed by the bridge and by
     * [dispatch]'s synthesised touches, never by the window's own dispatch (the ink surface is above it).
     */
    private class Entry(
        var spec: FrameSpec,
        val view: WebView,
    ) {
        var placement = FramePlacement(0f, 0f, 0f, 0f, 1f)
        var scroll = FrameScroll.NONE

        /** The URL this WebView was last told to load; what [FrameUrls.origin] compares before a reload. */
        var loadedUrl: String = spec.url

        /** The route the page itself last reported (or was sent to): a spec that matches it needs no reload. */
        var reportedRoute: String = spec.route

        /** Programmatic loads ignore the route messages of the document they replace. */
        var quietUntil: Long = 0L

        var pendingRoute: String? = null
        var routeRunnable: Runnable? = null
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
     * Opens a page at [spec]'s preset size. The WebView is laid out at the preset converted to device px, so its CSS
     * viewport is the preset width (the page's breakpoints are the preset's, at any canvas zoom); the canvas scale is a
     * view transform on top of that.
     */
    @SuppressLint("SetJavaScriptEnabled")
    private fun open(spec: FrameSpec): WebView {
        val view = WebView(context)
        with(view.settings) {
            javaScriptEnabled = true
            domStorageEnabled = true
            // The page is a document, not an app: no pinch inside it, no zoom buttons; the canvas owns the zoom.
            setSupportZoom(false)
            builtInZoomControls = false
            displayZoomControls = false
            // No useWideViewPort: the layout width is the view's width, so the page sees the preset as its viewport.
            useWideViewPort = false
            loadWithOverviewMode = false
            mixedContentMode = WebSettings.MIXED_CONTENT_NEVER_ALLOW
        }
        view.setBackgroundColor(Color.WHITE)
        view.isVerticalScrollBarEnabled = false
        view.isHorizontalScrollBarEnabled = false
        view.webViewClient = WebViewClient()
        view.addJavascriptInterface(FrameBridge { message -> deliver(spec.id, message) }, FrameMessages.HOST)
        view.layoutParams = LayoutParams((spec.cssWidth * density).toInt(), (spec.cssHeight * density).toInt())
        view.pivotX = 0f
        view.pivotY = 0f
        if (BuildConfig.DEBUG) WebView.setWebContentsDebuggingEnabled(true)
        view.loadUrl(spec.url)
        return view
    }

    /**
     * Points [entry] at [spec]: a page that already shows the spec's route (the user clicked a link; the item follows)
     * adopts it without a reload, anything else (another project, another host, a route typed by the user) loads.
     */
    private fun load(
        entry: Entry,
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
        entry: Entry,
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
                    Log.i(TAG, "$frameId hit ${message.key} '${message.selector}' ${message.rect}")
                }
            }
        }
    }

    /**
     * The page navigated by itself. Its item follows when the user stops: a menu that pushes several routes while it
     * settles must not queue an op each, and the page is already there, so the item update reloads nothing.
     */
    private fun queueRoute(
        entry: Entry,
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

    private fun commitRoute(entry: Entry) {
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
    }
}
