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
    fun the_open_round_is_the_one_with_status_open() {
        val json = """{"rounds":[{"n":3,"status":"resolved"},{"n":4,"status":"sent"},{"n":5,"status":"open"}]}"""
        assertEquals(5, OpenRoundJson.parse(json))
        assertNull(OpenRoundJson.parse("""{"rounds":[{"n":3,"status":"resolved"}]}"""))
        assertNull(OpenRoundJson.parse("nope"))
    }
}
