package dev.penwise.frames

import dev.penwise.ink.Clock
import dev.penwise.ink.PageRect
import dev.penwise.ink.PageStyle
import dev.penwise.ink.ViewportState
import dev.penwise.ink.lockTo
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** A locked frame is the screen: the camera follows every scroll of the live page, the pen excepted (§3.1). */
@Suppress("TooManyFunctions") // one rig, one test per rule
class LockedFollowTest {
    private class Target(
        override val spec: FrameSpec,
    ) : FollowTarget {
        override val moves = true
        override val pxPerCss = 1f
        override var cssX = 0f
        override var cssY = 0f
        override var needsAlign = false
        override val reflowing = false
        override val documentHeight = 6000f

        var onScrolled: ((Int) -> Unit)? = null

        override fun scrollToDevicePx(y: Int) {
            onScrolled?.invoke(y)
        }

        override fun stopFling() = Unit
    }

    private val spec =
        FrameSpec("f", "p", "/", "desktop", "http://x/", 0f, 0f, 1440, 900, full = true, boxHeight = 6000)
    private val target = Target(spec)
    private val viewport =
        ViewportState().apply {
            setPage(PageStyle("blank", 0f, 0f, infinite = true))
            setViewSize(1440f, 900f)
            lockTo(PageRect(0f, 0f, 1440f, 6000f))
        }
    private var penActive = false
    private var live = target
    private val follower =
        WindowFollower({ viewport }, { 900 }, { it === live }, { penActive }, {}, Clock { 1_000L })

    init {
        // The WebView reports every scroll, its own `scrollTo` included.
        target.onScrolled = { y -> follower.onScroll(target, 0, y) }
    }

    private fun scrollTo(cssY: Float) = follower.onScroll(target, 0, cssY.toInt())

    @Test
    fun a_scroll_the_page_did_by_itself_carries_the_camera_while_locked() {
        // An anchor link, scrollIntoView, a smooth-scroll library, a restored position: no finger.
        scrollTo(400f)
        assertEquals(-400f, viewport.transform.ty, 0.5f)
        assertEquals(400f, viewport.transform.toPageY(0f), 0.5f)
    }

    @Test
    fun with_the_pen_down_the_camera_stays_and_catches_up_when_it_lifts() {
        penActive = true
        scrollTo(300f)
        scrollTo(700f)
        assertEquals(0f, viewport.transform.ty, 0.5f)
        assertTrue(follower.owes)
        assertTrue("the pen is still down: try again", follower.catchUp())
        assertEquals(0f, viewport.transform.ty, 0.5f)
        penActive = false
        assertFalse(follower.catchUp())
        assertEquals(-700f, viewport.transform.ty, 0.5f)
        assertFalse(follower.owes)
    }

    @Test
    fun a_zoomed_in_camera_keeps_its_zoom_and_pans_by_the_scaled_scroll() {
        viewport.transformBy(2f, 0f, 0f, 0f, 0f)
        val scale = viewport.transform.scale
        val before = viewport.transform.toPageY(0f)
        scrollTo(100f)
        assertEquals(scale, viewport.transform.scale, 1e-4f)
        assertEquals(before + 100f, viewport.transform.toPageY(0f), 0.5f)
    }

    @Test
    fun an_unlocked_camera_still_follows_only_the_owner() {
        viewport.lock = null
        scrollTo(400f)
        assertEquals(0f, viewport.transform.ty, 0.5f)
        assertFalse(follower.owes)
    }

    @Test
    fun the_scroll_align_makes_does_not_carry_the_camera() {
        viewport.transformBy(1f, 0f, 0f, 0f, -3000f)
        val camera = viewport.transform
        follower.align(target)
        assertTrue(target.cssY > 1000f)
        assertEquals(camera, viewport.transform)
        assertFalse(follower.owes)
    }

    @Test
    fun the_scroll_align_makes_is_not_debt_under_the_pen() {
        viewport.transformBy(1f, 0f, 0f, 0f, -3000f)
        penActive = true
        follower.align(target)
        assertFalse(follower.owes)
    }

    @Test
    fun a_scroll_during_a_camera_flight_is_paid_back_not_lost() {
        viewport.driver = dev.penwise.ink.CameraDriver { }
        viewport.flyTo(viewport.transform.copy(ty = -50f), 200)
        assertTrue(viewport.animating)
        scrollTo(300f)
        assertTrue(follower.owes)
        assertTrue("still flying: try again", follower.catchUp())
        viewport.flight.cancel()
        viewport.driver = null
        assertFalse(follower.catchUp())
        assertEquals(300f, viewport.transform.toPageY(0f), 0.5f)
    }

    @Test
    fun a_scroll_during_a_dip_is_paid_back_after_it() {
        viewport.dip = 0.9f
        scrollTo(300f)
        assertTrue(follower.owes)
        viewport.dip = 1f
        assertFalse(follower.catchUp())
        assertEquals(300f, viewport.transform.toPageY(0f), 0.5f)
    }

    @Test
    fun an_align_echo_clamped_short_by_the_webview_is_still_the_echo() {
        viewport.transformBy(1f, 0f, 0f, 0f, -3000f)
        val camera = viewport.transform
        target.onScrolled = { y -> follower.onScroll(target, 0, y - 3) }
        follower.align(target)
        assertEquals(camera, viewport.transform)
        assertFalse(follower.owes)
    }

    @Test
    fun an_unrelated_scroll_does_not_use_up_the_align_expectation() {
        viewport.transformBy(1f, 0f, 0f, 0f, -3000f)
        val camera = viewport.transform
        var echo: (() -> Unit)? = null
        target.onScrolled = { y -> echo = { follower.onScroll(target, 0, y) } }
        follower.align(target)
        // an earlier, unrelated event (the other direction) arrives first
        follower.onScroll(target, 0, 0)
        val moved = viewport.transform
        echo?.invoke()
        assertEquals(moved, viewport.transform)
        assertTrue(moved != camera)
    }

    @Test
    fun debt_is_dropped_when_another_frame_becomes_live() {
        penActive = true
        scrollTo(300f)
        assertTrue(follower.owes)
        live = Target(spec)
        penActive = false
        val camera = viewport.transform
        assertFalse(follower.catchUp())
        assertEquals(camera, viewport.transform)
        assertFalse(follower.owes)
    }

    @Test
    fun debt_is_dropped_when_the_lock_changes() {
        penActive = true
        scrollTo(300f)
        viewport.lockTo(PageRect(0f, 100f, 1440f, 5000f))
        val camera = viewport.transform
        penActive = false
        assertFalse(follower.catchUp())
        assertEquals(camera, viewport.transform)
        assertFalse(follower.owes)
    }
}
