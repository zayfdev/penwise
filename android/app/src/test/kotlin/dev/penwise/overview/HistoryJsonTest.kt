package dev.penwise.overview

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** The history reply of contract §2.2 and the strip-pages reply of §2.3. */
class HistoryJsonTest {
    private val reply =
        """
        {"route":"/","preset":"desktop","generations":[
          {"round":3,"resolved_ms":1793550000000,
           "og":{"item":"aa","asset":"as1","w":1440,"h":7574},
           "variants":[{"label":"A","item":"bb","asset":"as2","w":1440,"h":7574,"picked":true},
                       {"label":"B","item":"cc","asset":"as3","w":1440,"h":7574,"picked":false}]},
          {"round":2,"resolved_ms":1793540000000,"og":{"item":"dd","asset":"as4","w":1440,"h":900},"variants":[]}
        ]}
        """.trimIndent()

    @Test
    fun generations_keep_the_servers_order_newest_first() {
        val parsed = HistoryJson.parse(reply) ?: error("no history")
        assertEquals(listOf(3, 2), parsed.map { it.round })
        assertEquals(1793550000000, parsed[0].resolvedMs)
    }

    @Test
    fun a_generation_carries_its_og_and_its_variants_with_the_pick() {
        val three = HistoryJson.parse(reply)?.first() ?: error("no history")
        assertEquals(HistoryPicture("aa", "as1", 1440, 7574), three.og)
        assertEquals(listOf("A", "B"), three.variants.map { it.label })
        assertEquals("A", three.picked?.label)
        assertEquals("as3", three.variants[1].picture.asset)
    }

    @Test
    fun a_generation_without_variants_has_no_pick() {
        val two = HistoryJson.parse(reply)?.last() ?: error("no history")
        assertTrue(two.variants.isEmpty())
        assertNull(two.picked)
    }

    @Test
    fun a_generation_without_an_og_picture_is_left_out() {
        val json = """{"generations":[{"round":1,"og":null,"variants":[]},{"round":2,"og":{"item":"a","asset":"b"}}]}"""
        assertEquals(listOf(2), HistoryJson.parse(json)?.map { it.round })
    }

    @Test
    fun a_variant_without_an_asset_is_left_out() {
        val og = """"og":{"item":"a","asset":"b"}"""
        val json = """{"generations":[{"round":1,$og,"variants":[{"label":"A","item":"c"}]}]}"""
        assertTrue(
            HistoryJson
                .parse(json)
                ?.first()
                ?.variants
                ?.isEmpty() == true,
        )
    }

    @Test
    fun anything_that_is_not_a_history_reply_is_null() {
        assertNull(HistoryJson.parse("not json"))
        assertNull(HistoryJson.parse("""{"error":"no"}"""))
        assertNotNull(HistoryJson.parse("""{"generations":[]}"""))
    }
}
