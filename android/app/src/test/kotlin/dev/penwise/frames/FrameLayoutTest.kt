package dev.penwise.frames

import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** The M2 bridge additions as the injected script sends them: `pw:layout`, `pw:replayed`, and the core's key list. */
class FrameLayoutTest {
    @Test
    fun layout_carries_scroll_doc_viewport_state_and_rects() {
        val json =
            """{"t":"pw:layout","scroll":[0,120.5],"doc":[390,3200],"viewport":[390,844],""" +
                """"state":"click:#menu","rects":{"v1||h1#hero":[24,180,342,160],"v1||gone":null}}"""
        val layout = FrameMessages.parse(json) as FrameMessage.Layout
        assertEquals(listOf(0f, 120.5f), layout.scroll)
        assertEquals(listOf(390f, 3200f), layout.doc)
        assertEquals(listOf(390f, 844f), layout.viewport)
        assertEquals("click:#menu", layout.state)
        assertEquals(FrameRect(24f, 180f, 342f, 160f), layout.rects["v1||h1#hero"])
        assertTrue(layout.rects.containsKey("v1||gone"))
        assertNull(layout.rects["v1||gone"])
    }

    @Test
    fun a_burst_layout_may_carry_only_the_changed_rects() {
        val json =
            """{"t":"pw:layout","scroll":[0,300],"doc":[390,3200],"viewport":[390,844],"state":"",""" +
                """"rects":{"v1||h1#hero":[24,140,342,160]}}"""
        val layout = FrameMessages.parse(json) as FrameMessage.Layout
        assertEquals(setOf("v1||h1#hero"), layout.rects.keys)
        assertEquals(FrameRect(24f, 140f, 342f, 160f), layout.rects["v1||h1#hero"])
    }

    @Test
    fun missing_layout_fields_decode_to_neutral_values() {
        val layout = FrameMessages.parse("""{"t":"pw:layout"}""") as FrameMessage.Layout
        assertTrue(layout.scroll.isEmpty())
        assertTrue(layout.doc.isEmpty())
        assertTrue(layout.rects.isEmpty())
        assertEquals("", layout.state)
    }

    @Test
    fun replayed_carries_the_state_and_whether_it_was_reached() {
        assertEquals(
            FrameMessage.Replayed("click:#menu", true),
            FrameMessages.parse("""{"t":"pw:replayed","state":"click:#menu","ok":true}"""),
        )
        assertEquals(
            FrameMessage.Replayed("", false),
            FrameMessages.parse("""{"t":"pw:replayed"}"""),
        )
    }

    @Test
    fun core_json_adds_the_preset_and_keeps_null_for_a_gone_element() {
        val layout =
            FrameMessage.Layout(
                scroll = listOf(0f, 120f),
                doc = listOf(390f, 3200f),
                viewport = listOf(390f, 844f),
                state = "click:#menu",
                rects = mapOf("v1||h1#hero" to FrameRect(24f, 180f, 342f, 160f), "v1||gone" to null),
            )
        val core = JSONObject(FrameLayouts.coreJson(layout, FramePreset.PHONE.key))
        assertEquals("phone", core.getString("preset"))
        assertEquals(0.0, core.getJSONArray("scroll").getDouble(0), 0.0)
        assertEquals(120.0, core.getJSONArray("scroll").getDouble(1), 0.0)
        assertEquals(390.0, core.getJSONArray("doc").getDouble(0), 0.0)
        assertEquals(3200.0, core.getJSONArray("doc").getDouble(1), 0.0)
        assertEquals("click:#menu", core.getString("state"))
        val rects = core.getJSONObject("rects")
        assertEquals(24.0, rects.getJSONArray("v1||h1#hero").getDouble(0), 0.0)
        assertEquals(160.0, rects.getJSONArray("v1||h1#hero").getDouble(3), 0.0)
        assertTrue(rects.isNull("v1||gone"))
    }

    @Test
    fun frame_keys_are_a_plain_string_array() {
        assertEquals(
            listOf("v1||h1#hero", "v1||nav > button"),
            FrameLayouts.keys("""["v1||h1#hero","v1||nav > button"]"""),
        )
        assertTrue(FrameLayouts.keys("[]").isEmpty())
        assertTrue(FrameLayouts.keys("nonsense").isEmpty())
        assertTrue(FrameLayouts.keys("""[""]""").isEmpty())
    }
}
