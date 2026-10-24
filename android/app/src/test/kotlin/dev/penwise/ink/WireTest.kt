package dev.penwise.ink

import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** Frames the tablet receives: page snapshots must parse back into exactly the strokes that were sent. */
class WireTest {
    private fun stroke(
        seed: Long,
        count: Int,
    ): SavedStroke =
        SavedStroke(
            presetIndex = 3,
            argb = -0x1000000,
            sizeScale = 1.25f,
            seed = seed,
            samples = FloatArray(count * 5) { it * 0.5f },
            times = IntArray(count) { it * 7 },
            count = count,
        )

    @Test
    fun a_stroke_survives_a_wire_round_trip() {
        val original = stroke(SEED, 4)
        val parsed = Wire.parseStroke(JSONObject(Wire.stroke(original).toString()))
        assertEquals(original.presetIndex, parsed.presetIndex)
        assertEquals(original.argb, parsed.argb)
        assertEquals(original.sizeScale, parsed.sizeScale, 1e-6f)
        assertEquals(original.seed, parsed.seed)
        assertEquals(original.count, parsed.count)
        assertEquals(original.samples.toList(), parsed.samples.toList())
        assertEquals(original.times.toList(), parsed.times.toList())
    }

    @Test
    fun a_page_frame_parses_into_its_strokes() {
        val frame = Wire.page(listOf(stroke(1, 2), stroke(2, 3)))
        val parsed = Wire.parsePage(frame)
        assertNotNull(parsed)
        assertEquals(listOf(1L, 2L), parsed!!.map { it.seed })
    }

    @Test
    fun other_frames_and_bad_json_are_ignored() {
        assertNull(Wire.parsePage("""{"t":"stroke","stroke":{}}"""))
        assertNull(Wire.parsePage("not json"))
        assertEquals(0, Wire.parsePage("""{"t":"page","strokes":[]}""")?.size)
    }

    @Test
    fun a_malformed_stroke_is_skipped_not_fatal() {
        val good = Wire.stroke(stroke(5, 2))
        val frame = """{"t":"page","strokes":[$good,{"preset":0,"argb":0,"seed":"7","samples":[1.0],"times":[1]}]}"""
        val parsed = Wire.parsePage(frame)
        assertEquals("the broken entry is dropped", 1, parsed?.size)
        assertEquals(5L, parsed?.first()?.seed)
    }

    private companion object {
        const val SEED = 9001L
    }
}
