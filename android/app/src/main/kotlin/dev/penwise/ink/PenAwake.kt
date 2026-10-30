package dev.penwise.ink

/** How long the screen stays awake after the last pen input. */
const val PEN_AWAKE_MS = 120_000L

/** A time source returning a primitive, so reading it never boxes (a `() -> Long` would). */
fun interface Clock {
    fun now(): Long
}

/**
 * Keeps the screen awake while the pen is in use and for [PEN_AWAKE_MS] after its last input (down, move, hover),
 * then lets the system timeout apply. Plain logic over an injected clock and one-shot timer so it runs without Android.
 *
 * [onPenInput] is on the per-sample path: it takes the event's own timestamp as a primitive, stores it and arms at most
 * one pending check, so it never allocates, boxes or posts per sample. [check] is what the timer fires; it re-arms for
 * the remainder if input arrived since. UI thread only.
 */
class PenAwake(
    private val clock: Clock,
    private val schedule: (delayMs: Long) -> Unit,
    private val cancel: () -> Unit,
    private val onChange: (awake: Boolean) -> Unit,
) : PenInputListener {
    private var lastInput = 0L
    private var awake = false
    private var pending = false

    /** The pen did something at [eventTime] (uptime ms): stay awake until [PEN_AWAKE_MS] later. */
    override fun onPenInput(eventTime: Long) {
        lastInput = eventTime
        if (!awake) {
            awake = true
            onChange(true)
        }
        if (!pending) {
            pending = true
            schedule(PEN_AWAKE_MS)
        }
    }

    /** The timer fired: sleep is allowed if the window has run out, else wait out what is left of it. */
    fun check() {
        pending = false
        val left = lastInput + PEN_AWAKE_MS - clock.now()
        if (left > 0) {
            pending = true
            schedule(left)
        } else if (awake) {
            awake = false
            onChange(false)
        }
    }

    /** The page was left: drop the timer and the wake request at once. */
    fun stop() {
        cancel()
        pending = false
        if (awake) {
            awake = false
            onChange(false)
        }
    }
}
