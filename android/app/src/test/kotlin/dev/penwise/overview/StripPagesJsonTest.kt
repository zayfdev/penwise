package dev.penwise.overview

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Test

/** The strip-pages reply of contract §2.3 (pages named by ULID) and the rounds reply. */
class StripPagesJsonTest {
    private val sketch1 = """"page":"$SKETCH_1_ULID""""
    private val sketch2 = """"page":"$SKETCH_2_ULID""""

    @Test
    fun strip_pages_parse_in_attach_order() {
        val json = """[{$sketch1,"route":"/","preset":"desktop"},{$sketch2,"route":"/about","preset":"phone"}]"""
        assertEquals(
            listOf(StripPage(SKETCH_1, "/", "desktop"), StripPage(SKETCH_2, "/about", "phone")),
            StripPagesJson.parse(json),
        )
    }

    @Test
    fun strip_pages_accept_a_wrapping_object_and_skip_broken_rows() {
        val lower = """"page":"${SKETCH_1_ULID.lowercase()}""""
        val json = """{"pages":[{$lower,"route":"/","preset":"phone"},{"route":"/"},{$sketch2}]}"""
        assertEquals(listOf(StripPage(SKETCH_1, "/", "phone")), StripPagesJson.parse(json))
    }

    @Test
    fun strip_pages_that_are_not_a_list_are_null() {
        assertNull(StripPagesJson.parse("nope"))
        assertFalse(StripPagesJson.parse("[]")?.isNotEmpty() == true)
    }

    @Test
    fun a_page_named_in_the_tablets_hex_form_is_not_a_server_id_and_is_skipped() {
        val hex = """"page":"$SKETCH_1""""
        val json = """[{$hex,"route":"/","preset":"desktop"},{"page":"p1","route":"/","preset":"desktop"}]"""
        assertEquals(emptyList<StripPage>(), StripPagesJson.parse(json))
    }

    @Test
    fun the_open_round_with_marks_is_the_now_round() {
        val json = """{"rounds":[{"n":1,"status":"sent","marks":1},{"n":2,"status":"open","marks":3}]}"""
        assertEquals(NowRound(2, sent = false), OpenRoundJson.parse(json))
    }

    @Test
    fun an_empty_open_round_after_a_send_leaves_the_now_column_on_the_sent_round() {
        val json =
            """{"rounds":[{"n":5,"status":"resolved","marks":2},{"n":6,"status":"sent","marks":3},""" +
                """{"n":7,"status":"open","marks":0}]}"""
        assertEquals(NowRound(6, sent = true), OpenRoundJson.parse(json))
    }

    @Test
    fun with_nothing_sent_an_empty_open_round_is_the_now_round() {
        val json = """{"rounds":[{"n":3,"status":"resolved","marks":1},{"n":4,"status":"open","marks":0}]}"""
        assertEquals(NowRound(4), OpenRoundJson.parse(json))
        assertNull(OpenRoundJson.parse("""{"rounds":[{"n":3,"status":"resolved","marks":1}]}"""))
        assertNull(OpenRoundJson.parse("nope"))
    }
}
