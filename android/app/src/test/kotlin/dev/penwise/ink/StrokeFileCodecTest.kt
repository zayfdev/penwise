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

/** Files written before 2026-10-25 interleaved each sample's time; reading them with the current layout loses ink. */
class LegacyStrokeLayoutTest {
    /** Writes one record the way the old builds did: `x, y, pressure, tilt, azimuth, time` per sample. */
    private fun legacyRecord(
        preset: Int,
        seed: Long,
        samples: FloatArray,
        times: IntArray,
    ): ByteArray {
        val buffer = ByteArrayOutputStream()
        DataOutputStream(buffer).use { out ->
            out.writeByte(1)
            out.writeInt(preset)
            out.writeInt(-16_777_216)
            out.writeFloat(1f)
            out.writeLong(seed)
            out.writeInt(times.size)
            for (i in times.indices) {
                for (k in 0 until 5) out.writeFloat(samples[i * 5 + k])
                out.writeInt(times[i])
            }
        }
        return buffer.toByteArray()
    }

    private val samples = floatArrayOf(10f, 20f, 0.5f, 0.1f, -1f, 12f, 24f, 0.6f, 0.2f, -1f)
    private val times = intArrayOf(0, 8_333)

    @Test
    fun an_interleaved_record_reads_back_exactly() {
        val bytes = legacyRecord(4, 7L, samples, times)
        val stroke = DataInputStream(ByteArrayInputStream(bytes)).use { SavedStrokeCodec.read(it) }
        assertNotNull(stroke)
        requireNotNull(stroke)
        assertEquals(4, stroke.presetIndex)
        assertEquals(samples.toList(), stroke.samples.take(10))
        assertEquals(times.toList(), stroke.times.toList())
    }

    @Test
    fun the_current_layout_still_reads() {
        val buffer = ByteArrayOutputStream()
        DataOutputStream(buffer).use { out ->
            SavedStrokeCodec.write(out, SavedStroke(2, -16_777_216, 1f, 9L, samples, times, times.size))
        }
        val stroke =
            DataInputStream(ByteArrayInputStream(buffer.toByteArray())).use { SavedStrokeCodec.read(it) }
        assertNotNull(stroke)
        requireNotNull(stroke)
        assertEquals(samples.toList(), stroke.samples.take(10))
        assertEquals(times.toList(), stroke.times.toList())
    }
}
