package dev.penwise.ink

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import kotlin.math.hypot

/** Finger lift and re-land never jump the view, and no input sequence leaves the gesture stuck on. */
class FingerGestureStateTest {
    private val viewport =
        ViewportState().apply {
            setPage(PageStyle("blank", 2560f, 1600f, infinite = false))
            setViewSize(2560f, 1600f)
        }
    private var taps = 0
    private val nav =
        FingerNavigation(
            SLOP,
            { taps++ },
            { viewport.transformBy(it.factor, it.fromX, it.fromY, it.toX, it.toY) },
            { viewport.setGesturing(it) },
        )

    private fun twoDown() {
        nav.down(0, 400f, 500f, 0)
        nav.down(1, 600f, 500f, 20)
    }

    private fun moveTo(
        x0: Float,
        y0: Float,
        x1: Float,
        y1: Float,
    ) {
        nav.move(0, x0, y0)
        nav.move(1, x1, y1)
        nav.update()
    }

    @Test
    fun lifting_one_finger_freezes_the_view_and_ends_the_gesture() {
        twoDown()
        moveTo(300f, 500f, 700f, 500f)
        nav.up(1, 200)
        assertFalse(viewport.gesturing)
        val frozen = viewport.transform
        nav.move(0, 900f, 900f)
        nav.update()
        assertEquals(frozen, viewport.transform)
        nav.up(0, 300)
        assertEquals(0, taps)
        assertEquals(frozen, viewport.transform)
    }

    @Test
    fun a_finger_that_lands_again_continues_without_a_jump() {
        twoDown()
        moveTo(300f, 500f, 700f, 500f)
        nav.up(1, 200)
        val before = viewport.transform
        nav.down(1, 1500f, 1200f, 400)
        assertEquals(before, viewport.transform)
        moveTo(300f, 500f, 1500f, 1200f)
        assertEquals("the re-landed finger's travel inside the slop does not move the page", before, viewport.transform)
        // From here the page point that was under the fingers' midpoint follows it, scale follows the span ratio.
        val anchorX = before.toPageX(900f)
        val anchorY = before.toPageY(850f)
        moveTo(300f, 500f, 1900f, 1200f)
        val view = viewport.transform
        val ratio = hypot(1600f, 700f) / hypot(1200f, 700f)
        assertEquals(before.scale * ratio, view.scale, 1e-3f)
        assertEquals(1100f, view.toScreenX(anchorX), 1e-1f)
        assertEquals(850f, view.toScreenY(anchorY), 1e-1f)
        assertTrue(viewport.gesturing)
    }

    @Test
    fun cancel_resets_the_gesture() {
        twoDown()
        moveTo(300f, 500f, 700f, 500f)
        assertTrue(viewport.gesturing)
        nav.cancel()
        assertFalse(viewport.gesturing)
        assertEquals(0, taps)
        // A fresh two-finger gesture works afterwards.
        twoDown()
        moveTo(300f, 500f, 700f, 500f)
        assertTrue(viewport.gesturing)
    }

    @Test
    fun fingers_missing_from_a_later_event_end_the_gesture_so_a_lost_up_cannot_stick() {
        twoDown()
        moveTo(300f, 500f, 700f, 500f)
        assertTrue(viewport.gesturing)
        nav.retain { it == 0 }
        assertFalse(viewport.gesturing)
        nav.retain { false }
        assertFalse(viewport.gesturing)
        twoDown()
        moveTo(300f, 500f, 700f, 500f)
        assertTrue("a new gesture is tracked from scratch", viewport.gesturing)
    }

    @Test
    fun a_third_finger_ends_navigation() {
        twoDown()
        moveTo(300f, 500f, 700f, 500f)
        nav.down(2, 100f, 100f, 100)
        assertFalse(viewport.gesturing)
        nav.up(2, 150)
        assertFalse(viewport.gesturing)
    }

    private companion object {
        const val SLOP = 24f
    }
}
