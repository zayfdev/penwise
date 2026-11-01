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
        assertEquals(500f, ScrollLink.cssOffset(2625, 2.625f * 2f), 1e-3f)
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

    private val tablet =
        FrameSpec("f", "p", "/", "tablet", "http://x/", 0f, 100f, 834, 1194, full = true, boxHeight = 5000)

    @Test
    fun the_window_is_never_drawn_past_the_documents_live_end() {
        // Tab S9 portrait, tablet preset: the WebView's offset rounded up to 3806.69 of a 3806 maximum, the window hung
        // off the screen, Chromium shrank the visual viewport and the offset ran on to 3816.
        val base = WindowBase(0f, 0f, 3806f, 3806f, moves = true)
        val live = ScrollLink.liveDocument(5000f, 5000f, 5000f)
        // Held at the live end, the window stays where the core put it for the real maximum scroll.
        assertEquals(100f, base.rect(tablet, 0f, 3816.07f, live).y, 0f)
        assertEquals(100f + 900f - 3806f, base.rect(tablet, 0f, 900f, live).y, 0f)
    }

    @Test
    fun a_box_that_has_not_reached_the_app_does_not_pin_the_window() {
        // boxHeight falls back to the preset height before the server's size arrives: the bridge's and the WebView's
        // own document height decide, and with none of them known nothing is clamped.
        val unknown = tablet.copy(boxHeight = 1194)
        val base = WindowBase(0f, 0f, 0f, 0f, moves = true)
        val fromBridge = ScrollLink.liveDocument(1194f, 5000f, 0f)
        assertEquals(100f + 2000f, base.rect(unknown, 0f, 2000f, fromBridge).y, 0f)
        val fromContent = ScrollLink.liveDocument(1194f, 0f, 5000f)
        assertEquals(100f + 2000f, base.rect(unknown, 0f, 2000f, fromContent).y, 0f)
        val nothing = ScrollLink.liveDocument(1194f, 0f, 0f)
        assertEquals(100f + 2000f, base.rect(unknown, 0f, 2000f, nothing).y, 0f)
    }

    @Test
    fun a_document_that_grew_past_the_box_is_followed() {
        val base = WindowBase(0f, 0f, 0f, 0f, moves = true)
        val live = ScrollLink.liveDocument(5000f, 5200f, 5400f)
        assertEquals(100f + 4000f, base.rect(tablet, 0f, 4000f, live).y, 0f)
    }
}
