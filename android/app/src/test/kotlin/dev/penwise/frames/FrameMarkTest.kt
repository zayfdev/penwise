package dev.penwise.frames

import dev.penwise.ink.ViewTransform
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** The core's mark JSON (`sceneMarks`) and where its chips land on screen (K1, K3). */
class FrameMarkTest {
    private val frameId = "00112233445566778899aabbccddeeff"
    private val markId = "ffeeddccbbaa99887766554433221100"

    private val phone = FrameSpec(frameId, "portfolio", "/", "phone", "http://127.0.0.1:17421/", 50f, 150f, 390, 844)

    @Test
    fun marks_carry_their_frame_preset_status_state_strokes_and_rect() {
        val json =
            """[{"id":"$markId","frame":"$frameId","preset":"phone","status":"reopened",""" +
                """"state":"click:#menu","strokes":["$markId"],"rect":[100,200,80,40]}]"""
        val mark = FrameMarks.parse(json).single()
        assertEquals(markId, mark.id)
        assertEquals(frameId, mark.frame)
        assertEquals("phone", mark.preset)
        assertEquals(MarkStatus.REOPENED, mark.status)
        assertEquals("click:#menu", mark.state)
        assertEquals(listOf(markId), mark.strokes)
        assertEquals(FrameRect(100f, 200f, 80f, 40f), mark.rect)
    }

    @Test
    fun a_hidden_mark_has_no_rect_and_an_unknown_status_is_new() {
        val json =
            """[{"id":"$markId","frame":"$frameId","preset":"phone","status":"blue","state":"",""" +
                """"strokes":[],"rect":null}]"""
        val mark = FrameMarks.parse(json).single()
        assertEquals(MarkStatus.NEW, mark.status)
        assertNull(mark.rect)
    }

    @Test
    fun malformed_mark_json_is_empty() {
        assertTrue(FrameMarks.parse("nonsense").isEmpty())
        assertTrue(FrameMarks.parse("""[{"id":"short","frame":"$frameId"}]""").isEmpty())
    }

    @Test
    fun chip_goes_to_the_marks_top_right_corner_in_screen_px() {
        val mark =
            SceneMark(
                id = markId,
                frame = frameId,
                preset = "phone",
                status = MarkStatus.NEW,
                state = "",
                strokes = emptyList(),
                rect = FrameRect(100f, 200f, 80f, 40f),
            )
        val chip = MarkPlacement.chips(listOf(mark), listOf(phone), emptyMap(), ViewTransform(2f, 10f, 20f)).single()
        // (100 + 80) * 2 + 10 = 370; 200 * 2 + 20 = 420.
        assertEquals(370f, chip.left, 0.01f)
        assertEquals(420f, chip.top, 0.01f)
        assertEquals(MarkStatus.NEW, chip.status)
    }

    @Test
    fun hidden_and_out_of_frame_marks_get_no_chip() {
        val hidden =
            SceneMark(markId, frameId, "phone", MarkStatus.DONE, "", emptyList(), rect = null)
        val outside =
            SceneMark(markId, frameId, "phone", MarkStatus.NEW, "", emptyList(), rect = FrameRect(600f, 200f, 40f, 40f))
        val onAnotherFrame =
            SceneMark(
                id = markId,
                frame = "ffffffffffffffffffffffffffffffff",
                preset = "phone",
                status = MarkStatus.NEW,
                state = "",
                strokes = emptyList(),
                rect = FrameRect(1f, 2f, 3f, 4f),
            )
        val chips =
            MarkPlacement.chips(
                listOf(hidden, outside, onAnotherFrame),
                listOf(phone),
                emptyMap(),
                ViewTransform.IDENTITY,
            )
        assertTrue(chips.isEmpty())
    }

    @Test
    fun a_mark_from_another_state_is_dimmed_until_that_state_is_active() {
        val mark =
            SceneMark(
                id = markId,
                frame = frameId,
                preset = "phone",
                status = MarkStatus.SEEN,
                state = "click:#menu",
                strokes = emptyList(),
                rect = FrameRect(100f, 200f, 80f, 40f),
            )
        val dimmed = MarkPlacement.chips(listOf(mark), listOf(phone), mapOf(frameId to ""), ViewTransform.IDENTITY)
        val active =
            MarkPlacement.chips(listOf(mark), listOf(phone), mapOf(frameId to "click:#menu"), ViewTransform.IDENTITY)
        assertTrue(dimmed.single().dimmed)
        assertTrue(!active.single().dimmed)
    }
}
