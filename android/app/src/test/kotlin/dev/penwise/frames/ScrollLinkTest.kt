package dev.penwise.frames

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** The scroll link (contract §5): window follows the page's scroll, the camera pans by the same amount. */
class ScrollLinkTest {
    @Test
    fun a_webview_scroll_in_device_px_is_css_px() {
        // 2.625 is the Tab S9's density class: 2625 device px are 1000 css px.
        assertEquals(1000f, ScrollLink.cssOffset(2625, 2.625f), 1e-3f)
        assertEquals(500f, ScrollLink.cssOffset(2625, 2.625f, pageZoom = 2f), 1e-3f)
    }

    private fun frame(
        x: Float,
        y: Float,
    ) = FrameSpec("f", "p", "/", "desktop", "http://x/", x, y, 1440, 900, full = true, boxHeight = 5000)

    @Test
    fun the_window_of_a_full_frame_slides_with_the_scroll_from_what_the_core_said() {
        // The core put the window 1000 below the frame's corner for a bridge scroll of 1000; the WebView is at 1300.
        val base = WindowBase(0f, 1000f, 0f, 1000f, moves = true)
        val at = frame(100f, 200f)
        assertEquals(1500f, base.rect(at, 0f, 1300f).y, EPS)
        assertEquals(100f, base.rect(at, 0f, 1300f).x, EPS)
        // Back at the scroll the core knew: exactly the core's window.
        assertEquals(1200f, base.rect(at, 0f, 1000f).y, EPS)
    }

    @Test
    fun the_window_of_a_viewport_frame_does_not_move() {
        val base = WindowBase(0f, 0f, 0f, 0f, moves = false)
        assertEquals(200f, base.rect(frame(100f, 200f), 0f, 900f).y, EPS)
    }

    @Test
    fun a_frame_that_moves_takes_its_window_along() {
        val base = WindowBase(0f, 1000f, 0f, 1000f, moves = true)
        val before = base.rect(frame(100f, 200f), 0f, 1300f)
        // The owner drags the frame, or a resolved round pushes it right.
        val after = base.rect(frame(1640f, 150f), 0f, 1300f)
        assertEquals(1540f, after.x - before.x, EPS)
        assertEquals(-50f, after.y - before.y, EPS)
        assertEquals(1640f, after.x, EPS)
    }

    @Test
    fun the_camera_pans_with_the_page_so_the_page_stays_fixed_on_the_board() {
        val scale = 2f
        val scrolled = 150f
        val pan = ScrollLink.cameraPan(scrolled, scale, moves = true)
        // Before: window top at screen y0. After the window slid by `scrolled` board px and the camera panned by pan.
        val before = 700f
        val after = before + scrolled * scale + pan
        assertEquals("the window stays where it is on the screen", before, after, EPS)
        assertEquals(0f, ScrollLink.cameraPan(scrolled, scale, moves = false), EPS)
    }

    @Test
    fun the_window_is_asked_to_cover_the_visible_part_clamped_to_the_document() {
        // A 900 px window, 800 px of the frame on screen from y 2000: centred on it.
        assertEquals(1950f, ScrollLink.coverScroll(2000f, 2800f, 900f, 6000f), EPS)
        // Visible part taller than the window: top aligned.
        assertEquals(2000f, ScrollLink.coverScroll(2000f, 3500f, 900f, 6000f), EPS)
        // Near the end of the document: clamped to docH - window.
        assertEquals(5100f, ScrollLink.coverScroll(5500f, 6000f, 900f, 6000f), EPS)
        // Before the start.
        assertEquals(0f, ScrollLink.coverScroll(0f, 400f, 900f, 6000f), EPS)
    }

    @Test
    fun a_window_that_already_covers_what_is_seen_is_left_alone() {
        assertTrue(ScrollLink.covers(1000f, 900f, 1100f, 1800f))
        assertFalse(ScrollLink.covers(1000f, 900f, 1100f, 2100f))
        assertFalse(ScrollLink.covers(1200f, 900f, 1100f, 1800f))
    }

    private companion object {
        const val EPS = 1e-3f
    }
}
