package dev.penwise.frames

import dev.penwise.ink.Clock
import dev.penwise.ink.PageStyle
import dev.penwise.ink.ViewTransform
import dev.penwise.ink.ViewportState
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** The scroll link: whose scrolls carry the camera, and whose only move the window (contract §1, §5). */
class WindowFollowerTest {
    private class Target(
        override val spec: FrameSpec,
    ) : FollowTarget {
        override val moves = true
        override val pxPerCss = DENSITY
        override var cssX = 0f
        override var cssY = 0f
        override var needsAlign = false
        override val documentHeight = 6000f
        var scrolledTo: Int? = null
        var flingStopped = 0

        override fun scrollToDevicePx(y: Int) {
            scrolledTo = y
        }

        override fun stopFling() {
            flingStopped++
        }
    }

    private val spec =
        FrameSpec("f", "p", "/", "desktop", "http://x/", 0f, 0f, 1440, 900, full = true, boxHeight = 6000)
    private val target = Target(spec)
    private val viewport =
        ViewportState().apply {
            setPage(PageStyle("blank", 0f, 0f, infinite = true))
            setViewSize(2560f, 1600f)
        }
    private var now = 1_000L
    private var penActive = false
    private var placed = 0
    private var moves = 0
    private val follower =
        WindowFollower(
            camera = { viewport },
            screenHeight = { 1600 },
            isLive = { true },
            penActive = { penActive },
            place = { placed++ },
            clock = Clock { now },
        )

    init {
        viewport.observers.add { moves++ }
    }

    /** The WebView scrolled to [cssY] CSS px. */
    private fun scrollTo(cssY: Float) = follower.onScroll(target, 0, (cssY * DENSITY).toInt())

    @Test
    fun a_scroll_by_the_owners_finger_pans_the_camera_with_the_page() {
        follower.fingerDown()
        scrollTo(100f)
        assertEquals(100f, target.cssY, 0.5f)
        // The camera moved down the page by the scroll, so the window slides over a page that stays on the board.
        assertEquals(-100f, viewport.transform.ty, 0.5f)
        assertEquals(1, moves)
    }

    @Test
    fun a_scroll_the_page_did_by_itself_moves_only_the_window() {
        // A reload restoring its position, an anchor, scrollIntoView, a smooth-scroll library: no finger.
        scrollTo(400f)
        assertEquals(ViewTransform.IDENTITY, viewport.transform)
        assertEquals(400f, target.cssY, 0.5f)
        assertEquals(1, placed)
        assertEquals(0, moves)
    }

    @Test
    fun the_fling_after_the_finger_lifts_still_carries_the_camera_until_it_stops() {
        follower.fingerDown()
        scrollTo(100f)
        now += 20
        follower.fingerUp()
        now += 16
        scrollTo(140f)
        now += 16
        scrollTo(170f)
        assertEquals(-170f, viewport.transform.ty, 0.5f)
        // The fling stopped; a later scroll (a script) is the page's own.
        now += 1_000
        scrollTo(900f)
        assertEquals(-170f, viewport.transform.ty, 0.5f)
        assertTrue(placed >= 1)
    }

    @Test
    fun a_tap_opens_no_fling_so_a_smooth_anchor_scroll_right_after_it_moves_only_the_window() {
        follower.fingerDown()
        now += 60
        follower.fingerUp()
        // The click made the page scroll to an anchor, smoothly, a few frames after the lift.
        now += 20
        scrollTo(100f)
        now += 16
        scrollTo(180f)
        now += 16
        scrollTo(240f)
        assertEquals(ViewTransform.IDENTITY, viewport.transform)
        assertEquals(240f, target.cssY, 0.5f)
        assertEquals(0, moves)
    }

    @Test
    fun a_scroll_that_comes_long_after_the_lift_is_not_a_fling_even_after_a_drag() {
        follower.fingerDown()
        scrollTo(100f)
        now += 20
        follower.fingerUp()
        // The finger rested before it lifted: no fling, and this smooth scroll is the page's own.
        now += 120
        scrollTo(300f)
        assertEquals(-100f, viewport.transform.ty, 0.5f)
    }

    @Test
    fun the_pen_never_carries_the_camera() {
        follower.fingerDown()
        penActive = true
        scrollTo(100f)
        assertEquals(ViewTransform.IDENTITY, viewport.transform)
        assertEquals(1, placed)
    }

    @Test
    fun the_pen_coming_down_stops_a_fling() {
        follower.fingerDown()
        scrollTo(100f)
        now += 20
        follower.fingerUp()
        now += 16
        follower.penDown(target)
        assertEquals(1, target.flingStopped)
        // Whatever the page still reports is not the finger's.
        now += 16
        scrollTo(130f)
        assertEquals(-100f, viewport.transform.ty, 0.5f)
        // And with no fling there is nothing to stop.
        follower.penDown(target)
        assertEquals(1, target.flingStopped)
    }

    @Test
    fun a_scroll_the_client_asked_for_to_cover_the_view_moves_only_the_window() {
        // The camera looks at the frame from y 3000 down; the page is at the top.
        viewport.transformBy(1f, 0f, 0f, 0f, -3000f)
        val before = viewport.transform
        follower.align(target)
        val asked = requireNotNull(target.scrolledTo)
        assertTrue(asked > 0)
        // The WebView reports the scroll it was asked to make: the owner's finger is not on it.
        follower.onScroll(target, 0, asked)
        assertEquals(before, viewport.transform)
        assertFalse(follower.linking)
    }

    private companion object {
        const val DENSITY = 2f
    }
}
