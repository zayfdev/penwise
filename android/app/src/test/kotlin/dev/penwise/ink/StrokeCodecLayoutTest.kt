package dev.penwise.ink

import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Test
import java.io.ByteArrayInputStream
import java.io.ByteArrayOutputStream
import java.io.DataInputStream
import java.io.DataOutputStream

/**
 * Short strokes (Line tool, tiny arrows) with zero tilt and azimuth used to be misread: the reader guessed a
 * second layout from the data and picked the wrong one. There is one layout; these must round-trip exactly.
 */
class StrokeCodecLayoutTest {
    private fun roundTrip(stroke: SavedStroke): SavedStroke {
        val buffer = ByteArrayOutputStream()
        DataOutputStream(buffer).use { SavedStrokeCodec.write(it, stroke) }
        return requireNotNull(DataInputStream(ByteArrayInputStream(buffer.toByteArray())).use(SavedStrokeCodec::read))
    }

    private fun stroke(
        samples: FloatArray,
        times: IntArray,
    ) = SavedStroke(3, -0x1000000, 1f, 42L, samples, times, times.size)

    @Test
    fun a_two_sample_line_round_trips_exactly() {
        val samples = floatArrayOf(800f, 600f, 0.6f, 0f, 0f, 1400f, 900f, 0.6f, 0f, 0f)
        val times = intArrayOf(0, 4000)
        val read = roundTrip(stroke(samples, times))
        assertArrayEquals(samples, read.samples, 0f)
        assertArrayEquals(times, read.times)
        assertEquals(2, read.count)
    }

    @Test
    fun a_three_sample_arrow_round_trips_exactly() {
        val samples =
            floatArrayOf(10f, 20f, 0.5f, 0f, 0f, 12f, 22f, 0.5f, 0f, 0f, 400f, 300f, 0.5f, 0f, 0f)
        val times = intArrayOf(0, 0, 8000)
        val read = roundTrip(stroke(samples, times))
        assertArrayEquals(samples, read.samples, 0f)
        assertArrayEquals(times, read.times)
    }
}
