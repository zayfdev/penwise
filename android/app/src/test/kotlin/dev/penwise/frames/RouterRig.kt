package dev.penwise.frames

/** A router over a fake host and a recording page, with a clock the test moves: shared by the touch routing tests. */
internal class RouterRig {
    inner class Host : GestureHost<String> {
        var fitted = false
        val doubleTaps = ArrayList<String>()
        var fingerDowns = 0
        var fingerUps = 0
        private val jobs = ArrayList<Pair<Runnable, Long>>()

        override fun liveAt(
            x: Float,
            y: Float,
        ): String = "page"

        override fun frameAt(
            x: Float,
            y: Float,
        ): String = "frame"

        override fun isFitted(page: String) = fitted

        override fun onDoubleTap(frameId: String) {
            doubleTaps += frameId
        }

        override fun fingerDown() {
            fingerDowns++
        }

        override fun fingerUp() {
            fingerUps++
        }

        override fun post(
            job: Runnable,
            delayMs: Long,
        ) {
            jobs += job to clock + delayMs
        }

        override fun remove(job: Runnable) {
            jobs.removeAll { it.first === job }
        }

        override fun now() = clock

        /** Lets [ms] pass, running what falls due in order. */
        fun advance(ms: Long) {
            val end = clock + ms
            while (true) {
                val next = jobs.filter { it.second <= end }.minByOrNull { it.second } ?: break
                jobs.remove(next)
                clock = maxOf(clock, next.second)
                next.first.run()
            }
            clock = end
        }
    }

    var clock = 1_000L
    val log = ArrayList<String>()
    val host = Host()
    val router =
        TouchRouter(
            host,
            PageSink<String> { _, action, _, _, x, y -> log += "${action.name} ${x.toInt()},${y.toInt()}" },
            slop = 20f,
        )

    fun down(
        x: Float,
        y: Float,
    ) = router.onDown(x, y, clock, clock)

    fun up(
        x: Float,
        y: Float,
    ) = router.onUp(x, y, clock)

    fun tap(
        x: Float,
        y: Float,
        holdMs: Long = 60,
    ) {
        down(x, y)
        host.advance(holdMs)
        up(x, y)
    }
}
