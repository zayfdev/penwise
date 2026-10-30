package dev.penwise.frames

/**
 * A repeating job that can be stopped and started again (the stills' poll, which must not run while the app is in the
 * background). Time is the scheduler's: [post] runs a runnable after a delay, [cancel] withdraws it. UI thread only.
 */
internal class PollLoop(
    private val periodMs: Long,
    private val post: (Runnable, delayMs: Long) -> Unit,
    private val cancel: (Runnable) -> Unit,
    private val work: () -> Unit,
) {
    var running = false
        private set

    private val tick =
        object : Runnable {
            override fun run() {
                if (!running) return
                work()
                post(this, periodMs)
            }
        }

    /** Starts at once (the first run is posted, not nested); starting a running loop does nothing. */
    fun start() {
        if (running) return
        running = true
        post(tick, 0L)
    }

    /** Stops; a run already posted is withdrawn. */
    fun stop() {
        running = false
        cancel(tick)
    }
}
