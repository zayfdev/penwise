package dev.penwise.frames

import org.junit.Assert.assertFalse

/** A router over a locked frame's still part, with a fake host that records the scrolls and flings. */
internal class StillRig {
    class Host : GestureHost<String> {
        var stillPage: String? = "page"
        val scrolls = ArrayList<Pair<Float, Float>>()
        val flings = ArrayList<Pair<Float, Float>>()
        var fingerDowns = 0
        var fingerUps = 0
        var doubleTaps = 0
        var clock = 1_000L
        var penRecent = false
        var hold = false

        override fun penRecent(time: Long) = penRecent

        override fun holdScroll() = hold

        override fun liveAt(
            x: Float,
            y: Float,
        ): String? = null

        override fun frameAt(
            x: Float,
            y: Float,
        ): String = "frame"

        override fun scrollableAt(
            x: Float,
            y: Float,
        ): String? = stillPage

        override fun scrollPage(
            page: String,
            dx: Float,
            dy: Float,
        ) {
            scrolls += dx to dy
        }

        override fun flingPage(
            page: String,
            vx: Float,
            vy: Float,
        ) {
            flings += vx to vy
        }

        override fun isFitted(page: String) = true

        override fun onDoubleTap(frameId: String) {
            doubleTaps++
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
        ) = Unit

        override fun remove(job: Runnable) = Unit

        override fun now() = clock
    }

    val host = Host()
    val router = TouchRouter(host, PageSink<String> { _, _, _, _, _, _ -> }, slop = 20f)

    fun drag(
        steps: Int,
        dyPerStep: Float,
        stepMs: Long = 10,
    ) {
        var y = 1_500f
        var t = host.clock
        assertFalse("the canvas still gets the touch-down", router.onDown(500f, y, t, t))
        repeat(steps) {
            y += dyPerStep
            t += stepMs
            router.onMove(500f, y, t)
        }
        host.clock = t
    }
}
