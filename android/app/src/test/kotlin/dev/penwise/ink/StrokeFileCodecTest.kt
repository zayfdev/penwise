package dev.penwise.ink

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Test
import java.io.ByteArrayInputStream
import java.io.ByteArrayOutputStream
import java.io.DataInputStream
import java.io.DataOutputStream
import java.io.EOFException

/** Page-file format: records survive a round trip, a torn tail keeps earlier strokes, garbage is rejected. */
class StrokeFileCodecTest {
    private fun stroke(
        seed: Long,
        samples: Int,
    ): SavedStroke =
        SavedStroke(
            presetIndex = PRESET,
            argb = ARGB,
            sizeScale = SIZE,
            seed = seed,
            samples = FloatArray(samples * STRIDE) { it * 0.25f },
            times = IntArray(samples) { it * 3 },
            count = samples,
        )

    private fun encode(vararg strokes: SavedStroke): ByteArray {
        val buffer = ByteArrayOutputStream()
        DataOutputStream(buffer).use { out -> strokes.forEach { SavedStrokeCodec.write(out, it) } }
        return buffer.toByteArray()
    }

    private fun readAll(bytes: ByteArray): List<SavedStroke> {
        val out = ArrayList<SavedStroke>()
        DataInputStream(ByteArrayInputStream(bytes)).use { input ->
            while (true) out += SavedStrokeCodec.read(input) ?: break
        }
        return out
    }

    @Test
    fun recordsRoundTripInOrder() {
        val strokes = readAll(encode(stroke(SEED_A, 3), stroke(SEED_B, 0), stroke(SEED_C, 1)))
        assertEquals(3, strokes.size)
        assertEquals(listOf(SEED_A, SEED_B, SEED_C), strokes.map { it.seed })
        val first = strokes[0]
        assertEquals(PRESET, first.presetIndex)
        assertEquals(ARGB, first.argb)
        assertEquals(SIZE, first.sizeScale, 0f)
        assertEquals(3 * STRIDE, first.samples.size)
        assertEquals("times[2] = 2 * 3", 6, first.times[2])
        assertEquals(0, strokes[1].count)
        assertNull("geometry is not built by the codec", first.geometry)
    }

    @Test
    fun cleanEndOfStreamReturnsNull() {
        DataInputStream(ByteArrayInputStream(ByteArray(0))).use { input ->
            assertNull(SavedStrokeCodec.read(input))
        }
    }

    @Test
    fun tornTailKeepsEarlierRecords() {
        val bytes = encode(stroke(SEED_A, 4), stroke(SEED_B, 4))
        val torn = bytes.copyOf(bytes.size - 6)
        val out = ArrayList<SavedStroke>()
        try {
            DataInputStream(ByteArrayInputStream(torn)).use { input ->
                while (true) out += SavedStrokeCodec.read(input) ?: break
            }
        } catch (_: EOFException) {
            // Expected: the second record is incomplete.
        }
        assertEquals(1, out.size)
        assertEquals(SEED_A, out[0].seed)
    }

    @Test
    fun unknownTagIsRejected() {
        val bytes = byteArrayOf(9)
        var failed = false
        try {
            DataInputStream(ByteArrayInputStream(bytes)).use { input -> SavedStrokeCodec.read(input) }
        } catch (_: IllegalStateException) {
            failed = true
        }
        assertEquals("a bad tag must not be read as a stroke", true, failed)
    }

    @Test
    fun rebuiltGeometryIsAttached() {
        val target = stroke(SEED_A, 2)
        assertEquals(null, target.geometry)
        assertNotNull(target)
    }

    private companion object {
        const val PRESET = 2
        const val ARGB = -0x1000000
        const val SIZE = 1.5f
        const val STRIDE = 5
        const val SEED_A = 11L
        const val SEED_B = 22L
        const val SEED_C = 33L
    }
}
