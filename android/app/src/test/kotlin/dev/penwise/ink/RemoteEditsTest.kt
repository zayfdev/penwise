package dev.penwise.ink

import org.json.JSONObject
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

/** Frames from other clients are untrusted: malformed or unbuildable strokes are skipped, never thrown. */
class RemoteEditsTest {
    private val warnings = ArrayList<String>()

    @Before
    fun capture() {
        RemoteEdits.warn = { warnings += it }
    }

    @After
    fun restore() {
        RemoteEdits.warn = {}
    }

    private fun wire(
        seed: Any = "123",
        preset: Int = 1,
    ) = JSONObject(
        """{"preset":$preset,"argb":-16777216,"scale":1.0,"seed":${if (seed is String) "\"$seed\"" else seed},
            |"samples":[1,2,3,0,0,4,5,6,0,0],"times":[0,10]}
        """.trimMargin(),
    )

    private val noGeometry: (SavedStroke) -> Unit = { if (it.presetIndex == BAD_PRESET) error("out of range") }

    @Test
    fun a_wellformed_stroke_parses() {
        assertEquals(123L, RemoteEdits.rebuiltStroke(wire(), noGeometry)?.seed)
    }

    @Test
    fun a_non_numeric_seed_is_skipped_with_a_warning() {
        assertNull(RemoteEdits.rebuiltStroke(wire(seed = "not-a-number"), noGeometry))
        assertEquals(1, warnings.size)
    }

    @Test
    fun a_stroke_missing_its_fields_is_skipped() {
        assertNull(RemoteEdits.rebuiltStroke(JSONObject("""{"seed":"5"}"""), noGeometry))
        assertTrue(warnings.isNotEmpty())
    }

    @Test
    fun an_out_of_range_preset_is_skipped_not_thrown() {
        assertNull(RemoteEdits.rebuiltStroke(wire(preset = BAD_PRESET), noGeometry))
        assertEquals(1, warnings.size)
    }

    @Test
    fun a_malformed_op_frame_yields_an_op_without_the_bad_strokes() {
        val op = JSONObject("""{"k":"add","stroke":{"seed":"x"}}""")
        val read = RemoteEdits.read(op, noGeometry)
        assertNotNull(read)
        assertEquals(RemoteOp.Add(emptyList()), read)
        val replace =
            JSONObject("""{"k":"replace","seeds":["1"],"strokes":[{"bogus":1}]}""")
        assertEquals(RemoteOp.Replace(setOf(1L), emptyList()), RemoteEdits.read(replace, noGeometry))
    }

    private companion object {
        const val BAD_PRESET = 99
    }
}
