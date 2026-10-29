package dev.penwise.frames

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/** The bridge wire format as the injected script sends it (`pw:hello`, `pw:scroll`, `pw:hit`). */
class FrameMessageTest {
    @Test
    fun hello_carries_the_document_and_the_viewport() {
        val json = """{"t":"pw:hello","url":"http://127.0.0.1:17421/","docW":390,"docH":3240,"vw":390,"vh":844}"""
        assertEquals(
            FrameMessage.Hello("http://127.0.0.1:17421/", 390, 3240, 390, 844),
            FrameMessages.parse(json),
        )
    }

    @Test
    fun scroll_carries_the_offset_and_the_document_size() {
        val json = """{"t":"pw:scroll","x":0,"y":1280,"docW":390,"docH":3240}"""
        assertEquals(FrameMessage.Scroll(0, 1280, 390, 3240), FrameMessages.parse(json))
    }

    @Test
    fun hit_carries_the_element_fingerprint_and_its_rect() {
        val json =
            """{"t":"pw:hit","id":7,"key":"h1#hero.hero-title:nth-of-type(1)","selector":"h1.hero-title",""" +
                """"rect":[24,180,342,160]}"""
        assertEquals(
            FrameMessage.Hit(7, "h1#hero.hero-title:nth-of-type(1)", "h1.hero-title", listOf(24f, 180f, 342f, 160f)),
            FrameMessages.parse(json),
        )
    }

    @Test
    fun unknown_types_are_dropped() {
        assertNull(FrameMessages.parse("""{"t":"pw:patchApplied"}"""))
        assertNull(FrameMessages.parse("""{"url":"http://127.0.0.1:17421/"}"""))
    }

    @Test
    fun broken_json_is_dropped_instead_of_throwing() {
        assertNull(FrameMessages.parse("not json at all"))
        assertNull(FrameMessages.parse(""))
    }

    @Test
    fun missing_fields_decode_to_their_neutral_values() {
        assertEquals(FrameMessage.Scroll(0, 0, 0, 0), FrameMessages.parse("""{"t":"pw:scroll"}"""))
        assertEquals(FrameMessage.Hit(-1, "", "", emptyList()), FrameMessages.parse("""{"t":"pw:hit"}"""))
    }

    @Test
    fun route_carries_the_page_own_navigation() {
        val json = """{"t":"pw:route","route":"/work?tag=web"}"""
        assertEquals(FrameMessage.Route("/work?tag=web"), FrameMessages.parse(json))
        assertEquals(FrameMessage.Route("/"), FrameMessages.parse("""{"t":"pw:route"}"""))
    }
}
