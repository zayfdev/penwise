package dev.penwise.frames

import kotlin.math.hypot

/** What the router sends to a page: a single-pointer touch event, in window coordinates. */
internal enum class TouchAction { DOWN, MOVE, UP, CANCEL }

/** The page behind a [TouchRouter]: the WebView in the app, a recorder in tests. */
internal fun interface PageSink<T : Any> {
    fun send(
        target: T,
        action: TouchAction,
        time: Long,
        downTime: Long,
        x: Float,
        y: Float,
    )
}

/** What the touch routing needs to know about the frames. */
internal interface GestureHost<T : Any> : StillScrollHost<T> {
    /** The live page when a window point ([x], [y]) is on its window; null otherwise. */
    fun liveAt(
        x: Float,
        y: Float,
    ): T?

    /** The frame whose box holds the window point, topmost first; null off every frame. */
    fun frameAt(
        x: Float,
        y: Float,
    ): String?

    /** The live page already fills the screen width: it owns its double-taps. */
    fun isFitted(page: T): Boolean

    /** A one-finger double-tap on [frameId]'s box: fit it to the screen. */
    fun onDoubleTap(frameId: String)

    /** Runs [job] after [delayMs] on the UI thread, or withdraws it. */
    fun post(
        job: Runnable,
        delayMs: Long,
    )

    fun remove(job: Runnable)

    /** The clock the events of this host are stamped with (uptime ms). */
    fun now(): Long
}

/**
 * The finger routing of the frame layer, free of Android types: one finger on the live window belongs to its page
 * (single-pointer events kept native: scrolling, clicking, selecting), a second finger hands the whole gesture to the
 * canvas, and a one-finger double-tap on any frame's box asks the host to fit it. Everything else is the canvas's.
 *
 * A live page that is not fitted yet does not see a touch right away. A drag reaches it from its first point. A touch
 * that stays down past [DoubleTap.TAP_TIMEOUT_MS] without moving is a press, not a tap: it is handed over from its
 * first point and the page owns the gesture (long-press, selection, `:active`). A quick tap waits [DoubleTap.GAP_MS]
 * for a second one: two taps in a row fit the frame and the page saw neither, otherwise the page gets the tap late
 * ([TapReplay]). Once the page fills the screen it owns its double-taps.
 */
internal class TouchRouter<T : Any>(
    private val host: GestureHost<T>,
    private val sink: PageSink<T>,
    private val slop: Float,
) {
    private var mode = Mode.IDLE
    private var active: T? = null
    private var downTime = 0L
    private var lastX = 0f
    private var lastY = 0f
    private val taps = DoubleTap(slop)

    /** The touch that is held back until it is a tap, a press or a drag: where it began, on which page. */
    private var held: T? = null
    private var heldX = 0f
    private var heldY = 0f
    private var heldDownTime = 0L
    private val replay = TapReplay(host, sink, slop * 2)
    private val press = Runnable { flushHeld() }

    /** A finger on the still part of the locked frame. */
    private val still = StillScroll(host, slop)

    /** A finger is working the live page right now. */
    val touching: Boolean get() = active != null

    /** Anything at all is held, active or waiting to be replayed. */
    val busy: Boolean get() = (mode != Mode.IDLE && mode != Mode.CANVAS) || replay.pending

    /** The first finger went down at ([x], [y]); true when the page took the touch (held or forwarded). */
    fun onDown(
        x: Float,
        y: Float,
        time: Long,
        downTime: Long,
    ): Boolean {
        if (mode != Mode.IDLE) {
            drop()
            mode = Mode.IDLE
        }
        taps.down(host.frameAt(x, y), x, y, time)
        // A tap far from the one waiting is a different tap: the waiting one reaches its page now.
        replay.onNextDown(x, y)
        val page = host.liveAt(x, y)
        if (page == null) {
            if (still.down(x, y, time)) mode = Mode.STILL
            return false
        }
        // A new touch ends a fling the page is in, as a touch on a browser does (a held touch is not sent yet).
        host.flingPage(page, 0f, 0f)
        if (!host.isFitted(page)) {
            held = page
            heldX = x
            heldY = y
            heldDownTime = downTime
            mode = Mode.HELD
            host.post(press, DoubleTap.TAP_TIMEOUT_MS)
            return true
        }
        replay.settle(doubled = false)
        active = page
        this.downTime = downTime
        host.fingerDown()
        send(page, TouchAction.DOWN, time, x, y)
        mode = Mode.FRAME
        return true
    }

    /**
     * Two fingers (or a palm and a finger): the canvas takes the gesture. The page gets a cancel if it had the touch,
     * and never sees a touch it only held. True when there was a gesture to take over.
     */
    fun takeForCanvas(): Boolean {
        taps.spoil()
        if (mode == Mode.IDLE || mode == Mode.CANVAS) return false
        drop()
        mode = Mode.CANVAS
        return true
    }

    /** The finger moved to ([x], [y]); true when the page has the gesture. */
    fun onMove(
        x: Float,
        y: Float,
        time: Long,
    ): Boolean {
        taps.move(x, y)
        return when (mode) {
            Mode.HELD -> {
                if (hypot(x - heldX, y - heldY) > slop) {
                    host.remove(press)
                    flushHeld()
                    send(active ?: return true, TouchAction.MOVE, time, x, y)
                }
                true
            }

            Mode.FRAME -> {
                active?.let { send(it, TouchAction.MOVE, time, x, y) }
                true
            }

            Mode.STILL -> {
                still.move(x, y, time)
            }

            else -> {
                false
            }
        }
    }

    /** The last finger lifted at ([x], [y]); true when the page had the gesture. */
    fun onUp(
        x: Float,
        y: Float,
        time: Long,
    ): Boolean {
        val doubled = taps.up(x, y, time)
        doubled?.let(host::onDoubleTap)
        val had = mode
        if (had == Mode.HELD) {
            host.remove(press)
            val page = held
            held = null
            // The first tap of a double-tap is dropped with the second; otherwise it is delivered.
            replay.settle(doubled != null)
            if (doubled == null && page != null) replay.schedule(page, heldX, heldY)
        } else if (had == Mode.FRAME) {
            active?.let { send(it, TouchAction.UP, time, x, y) }
            active = null
            host.fingerUp()
        } else {
            // A touch that was not the page's may still have suspended a waiting tap: it is dropped or delivered now.
            replay.settle(doubled != null)
        }
        // A STILL touch that never scrolled is a tap the canvas also sees, as ever.
        val scrolled = had == Mode.STILL && still.up(time)
        mode = Mode.IDLE
        return had == Mode.HELD || had == Mode.FRAME || scrolled
    }

    /** The system cancelled the touch. */
    fun onCancel(): Boolean {
        taps.reset()
        val had = mode
        val scrolled = had == Mode.STILL && still.scrolling
        drop()
        mode = Mode.IDLE
        return had == Mode.HELD || had == Mode.FRAME || scrolled
    }

    /**
     * The pen touched down: a finger gesture in flight ends, and no tap sequence survives. A tap waiting to be
     * replayed still is: a pen hovering or writing elsewhere does not take a finger's tap away.
     */
    fun penDown() {
        taps.reset()
        if (mode == Mode.IDLE || mode == Mode.CANVAS) return
        drop()
        mode = Mode.IDLE
    }

    /** [page] is being closed: nothing held, active or waiting for it survives. */
    fun release(page: T) {
        if (active === page || held === page || still.page === page) {
            drop()
            mode = Mode.IDLE
        }
        replay.cancel(page)
    }

    /** Back to idle without a cancel (the layer is gone). */
    fun reset() {
        // The owner scroll ends with the layer: ScrollOwner must not stay touching after a page leave.
        if (mode == Mode.FRAME) host.fingerUp()
        if (mode == Mode.STILL) still.drop()
        held = null
        active = null
        still.reset()
        host.remove(press)
        replay.cancel()
        taps.reset()
        mode = Mode.IDLE
    }

    /** The held touch is a drag or a press: the page gets it from where it began. */
    private fun flushHeld() {
        val page = held ?: return
        held = null
        replay.settle(doubled = false)
        active = page
        downTime = heldDownTime
        host.fingerDown()
        send(page, TouchAction.DOWN, heldDownTime, heldX, heldY)
        mode = Mode.FRAME
    }

    /** Ends whatever the page had of the touch: a cancel if it was forwarding, nothing if it was only held. */
    private fun drop() {
        host.remove(press)
        held = null
        replay.settle(doubled = false)
        if (mode == Mode.STILL) {
            still.drop()
            return
        }
        val page = active ?: return
        send(page, TouchAction.CANCEL, host.now(), lastX, lastY)
        active = null
        host.fingerUp()
    }

    private fun send(
        page: T,
        action: TouchAction,
        time: Long,
        x: Float,
        y: Float,
    ) {
        lastX = x
        lastY = y
        sink.send(page, action, time, downTime, x, y)
    }

    private enum class Mode {
        /** Nothing of ours is down. */
        IDLE,

        /** One finger is working a page. */
        FRAME,

        /** One finger is down on a page that is not fitted: a tap, a press or a drag, not decided yet. */
        HELD,

        /** One finger is down on the still part of the locked frame: a tap, or a page scroll once it drags. */
        STILL,

        /** A gesture that began on a page became a canvas gesture; the canvas owns it until every finger is up. */
        CANVAS,
    }
}
