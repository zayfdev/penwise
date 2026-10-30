package dev.penwise.frames

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** The still manifest (contract §4) and which strips a view needs. */
class StillStripsTest {
    private val json =
        """{"version":7,"doc":[1440,9000],"viewport":[1440,900],"strips":[""" +
            """{"y":0,"h":4096,"url":"/frames/f/still/7/0.png"},""" +
            """{"y":4096,"h":4096,"url":"/frames/f/still/7/4096.png"},""" +
            """{"y":8192,"h":808,"url":"/frames/f/still/7/8192.png"}],""" +
            """"layout":{"rects":{"v1||h1":[24,5200,342,160],"v1||gone":null},"state":"click:#menu"}}"""

    @Test
    fun the_manifest_reads_sizes_strips_and_the_layout() {
        val manifest = requireNotNull(StillManifests.parse(json))
        assertEquals("7", manifest.version)
        assertEquals(1440, manifest.docWidth)
        assertEquals(9000, manifest.docHeight)
        assertEquals(3, manifest.strips.size)
        assertEquals(StillStrip(4096, 4096, "/frames/f/still/7/4096.png"), manifest.strips[1])
        assertEquals(FrameRect(24f, 5200f, 342f, 160f), manifest.rects["v1||h1"])
        assertNull(manifest.rects["v1||gone"])
        assertEquals("click:#menu", manifest.state)
    }

    @Test
    fun the_layout_a_still_gives_the_core_is_scrolled_to_the_top_in_document_coordinates() {
        val layout = requireNotNull(StillManifests.parse(json)).asLayout()
        assertEquals(listOf(0f, 0f), layout.scroll)
        assertEquals(listOf(1440f, 9000f), layout.doc)
        assertEquals(listOf(1440f, 900f), layout.viewport)
        assertEquals(FrameRect(24f, 5200f, 342f, 160f), layout.rects["v1||h1"])
    }

    @Test
    fun garbage_and_versionless_manifests_are_no_still() {
        assertNull(StillManifests.parse("nope"))
        assertNull(StillManifests.parse("""{"doc":[1,1],"strips":[]}"""))
    }

    @Test
    fun only_the_strips_that_overlap_the_visible_rows_are_chosen() {
        val strips = requireNotNull(StillManifests.parse(json)).strips
        assertEquals(listOf(0), StillStrips.visible(strips, 0f, 900f).map { it.y })
        assertEquals(listOf(0, 4096), StillStrips.visible(strips, 3000f, 5000f).map { it.y })
        assertEquals(listOf(4096), StillStrips.visible(strips, 4200f, 5000f).map { it.y })
        assertEquals(listOf(8192), StillStrips.visible(strips, 8300f, 8900f).map { it.y })
        // The margin pulls in the neighbour just outside the view.
        assertEquals(listOf(0, 4096), StillStrips.visible(strips, 4200f, 5000f, margin = 200f).map { it.y })
        assertTrue(StillStrips.visible(strips, 20_000f, 21_000f).isEmpty())
    }

    @Test
    fun strips_are_decoded_at_screen_resolution_not_full_size() {
        // A 2880 px wide strip shown 360 px wide (a page seen from far away): shrink by 8.
        assertEquals(8, StillStrips.sampleSize(2880, 360))
        // Shown at full size or zoomed in: not shrunk.
        assertEquals(1, StillStrips.sampleSize(2880, 2880))
        assertEquals(1, StillStrips.sampleSize(2880, 5000))
        // Never below the target.
        assertEquals(2, StillStrips.sampleSize(2880, 1000))
    }

    @Test
    fun the_servers_hit_answer_is_in_document_px() {
        val hit =
            requireNotNull(
                StillClient.parseHit("""{"t":"pw:hit","key":"v1||h1","selector":"h1","rect":[24,5200,342,160]}"""),
            )
        assertEquals("v1||h1", hit.key)
        assertEquals(listOf(24f, 5200f, 342f, 160f), hit.rect)
        assertTrue(hit.document)
        assertNull(StillClient.parseHit("""{"key":"","rect":[]}"""))
    }
}
