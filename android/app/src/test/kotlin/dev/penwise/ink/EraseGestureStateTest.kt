package dev.penwise.ink

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test

/** The gesture state the controller resets on `ACTION_CANCEL`, and the erase-path helpers it relies on. */
class EraseGestureStateTest {
    private class Probe : AutoCloseable {
        var closed = false

        override fun close() {
            closed = true
        }
    }

    private fun saved(count: Int) = SavedStroke(0, 0, 1f, 1L, FloatArray(count * 5), IntArray(count), count)

    @Test
    fun abandoning_a_shape_drag_forgets_the_start_and_closes_the_preview() {
        val drag = ShapeDrag<Probe>()
        val preview = Probe()
        drag.begin(10f, 20f)
        drag.replaceTemp(preview)
        assertTrue(drag.active)

        drag.abandon()

        assertFalse("the next pen-down must not see a half-finished shape", drag.active)
        assertNull(drag.start)
        assertTrue(preview.closed)
        assertNull("nothing is left to commit", drag.finish())
    }

    @Test
    fun a_new_drag_replaces_and_closes_the_previous_preview() {
        val drag = ShapeDrag<Probe>()
        val first = Probe()
        val second = Probe()
        drag.begin(0f, 0f)
        drag.replaceTemp(first)
        drag.replaceTemp(second)
        assertTrue(first.closed)
        assertFalse(second.closed)
        assertSame(second, drag.finish())
        assertFalse(drag.active)
    }

    private fun encode(vararg fragments: List<Float>): IntArray {
        val flat = ArrayList<Int>()
        flat += fragments.size
        for (f in fragments) {
            flat += f.size / 6
            for (i in f.indices) flat += if (i % 6 == 5) f[i].toInt() else f[i].toRawBits()
        }
        return flat.toIntArray()
    }

    @Test
    fun a_native_untouched_marker_means_no_replacement() {
        assertTrue(PreciseErase.decode(saved(2), intArrayOf(-1)) === EraseCut.Unchanged)
    }

    @Test
    fun native_fragments_decode_into_strokes_of_the_same_brush() {
        val flat = encode(listOf(0f, 1f, 0.5f, 0f, 0f, 10f, 5f, 2f, 0.7f, 0f, 0f, 20f))
        val cut = PreciseErase.decode(saved(2), flat) as EraseCut.Pieces
        assertEquals(1, cut.pieces.size)
        val piece = cut.pieces[0]
        assertEquals(2, piece.count)
        assertEquals(5f, piece.samples[5], 0f)
        assertEquals(0.7f, piece.samples[7], 0f)
        assertEquals(listOf(10, 20), piece.times.toList())
    }

    @Test
    fun zero_native_fragments_means_the_stroke_is_removed() {
        val cut = PreciseErase.decode(saved(2), intArrayOf(0)) as EraseCut.Pieces
        assertTrue(cut.pieces.isEmpty())
    }

    @Test
    fun erase_frames_go_out_once_per_interval_with_the_newest_point() {
        val sent = ArrayList<Pair<Float, Float>>()
        val throttle = EraseFrameThrottle(16) { x, y -> sent += x to y }
        throttle.point(1f, 1f, 1_000)
        throttle.point(2f, 2f, 1_002)
        throttle.point(3f, 3f, 1_004)
        assertEquals(listOf(1f to 1f), sent)

        throttle.point(4f, 4f, 1_016)
        assertEquals(listOf(1f to 1f, 4f to 4f), sent)
    }

    @Test
    fun flush_sends_the_held_point_once() {
        val sent = ArrayList<Pair<Float, Float>>()
        val throttle = EraseFrameThrottle(16) { x, y -> sent += x to y }
        throttle.point(1f, 1f, 1_000)
        throttle.point(9f, 9f, 1_001)
        throttle.flush()
        throttle.flush()
        assertEquals(listOf(1f to 1f, 9f to 9f), sent)
    }

    @Test
    fun reset_starts_a_new_gesture_without_a_stale_point() {
        val sent = ArrayList<Pair<Float, Float>>()
        val throttle = EraseFrameThrottle(16) { x, y -> sent += x to y }
        throttle.point(1f, 1f, 1_000)
        throttle.point(9f, 9f, 1_001)
        throttle.reset()
        throttle.flush()
        throttle.point(5f, 5f, 1_002)
        assertEquals(listOf(1f to 1f, 5f to 5f), sent)
    }
}
