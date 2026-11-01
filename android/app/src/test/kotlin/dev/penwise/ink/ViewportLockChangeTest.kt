package dev.penwise.ink

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** The camera locked to a frame's box: changing, releasing and sliding between locks. */
class ViewportLockChangeTest {
    private val infinite = PageStyle("blank", 0f, 0f, infinite = true)

    // A 1440 x 4000 box at (2000, 100) on a 2880 x 1600 screen: fit-width scale 2.
    private val box = PageRect(2000f, 100f, 1440f, 4000f)

    private fun locked(): ViewportState =
        ViewportState().apply {
            setPage(infinite)
            setViewSize(2880f, 1600f)
            lockTo(box)
        }

    @Test
    fun a_reset_fits_the_locked_box_not_the_page() {
        val state = locked()
        state.transformBy(3f, 0f, 0f, -400f, -400f)
        state.reset()
        assertEquals(2f, state.transform.scale, EPS)
        assertEquals(2000f, state.transform.toPageX(0f), EPS)
        assertEquals(100f, state.transform.toPageY(0f), EPS)
    }

    @Test
    fun a_new_page_style_drops_the_lock() {
        val state = locked()
        state.setPage(PageStyle("blank", 2560f, 1600f, infinite = false))
        assertFalse(state.locked)
    }

    @Test
    fun a_slide_between_frames_takes_at_most_180_ms() {
        val from = ViewTransform(2f, -4000f, -200f)
        val to = ViewTransform(2f, -10_000f, -200f)
        val tween = CameraTween(from, to, 2880f, 1600f, durationMs = 160L)
        assertEquals(from.tx, tween.at(0L).tx, EPS)
        assertTrue(tween.at(80L).tx < from.tx)
        assertEquals(to, tween.at(160L))
        assertTrue(tween.durationMs <= 180L)
    }

    @Test
    fun a_flight_runs_for_the_tweens_own_duration() {
        var last: ViewTransform? = null
        val flight = CameraFlight { last = it }
        val to = ViewTransform(2f, -10_000f, 0f)
        flight.start(CameraTween(ViewTransform(2f, 0f, 0f), to, 2880f, 1600f, durationMs = 100L))
        assertTrue(flight.step(1_000L))
        assertFalse(flight.step(1_100L))
        assertEquals(to, last)
    }

    @Test
    fun unlocking_frees_the_zoom_again() {
        val state = locked()
        state.unlock()
        assertFalse(state.locked)
        state.transformBy(0.1f, 0f, 0f, 0f, 0f)
        assertEquals(0.2f, state.transform.scale, EPS)
    }

    @Test
    fun a_box_that_changes_size_pulls_the_camera_back_inside() {
        val state = locked()
        state.transformBy(1f, 0f, 0f, 0f, -6000f)
        state.relock(PageRect(2000f, 100f, 1440f, 1000f))
        assertEquals(1100f, state.transform.toPageY(1600f), EPS)
    }

    @Test
    fun locking_to_another_box_moves_the_limits_with_it() {
        val state = locked()
        state.lockTo(PageRect(5000f, 100f, 1440f, 4000f))
        assertEquals(5000f, state.transform.toPageX(0f), EPS)
        state.transformBy(1f, 0f, 0f, 90_000f, 0f)
        assertEquals(5000f, state.transform.toPageX(0f), EPS)
    }

    private companion object {
        const val EPS = 1e-2f
    }
}
