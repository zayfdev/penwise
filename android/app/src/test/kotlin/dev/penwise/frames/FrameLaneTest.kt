package dev.penwise.frames

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/** The lane's live frame is the server's pick (rightmost plain frame of the route and preset), variants by label. */
class FrameLaneTest {
    private fun spec(
        id: String,
        x: Float,
        route: String = "/",
        preset: String = "desktop",
        project: String = "p",
        label: String = "",
        group: String? = null,
    ) = FrameSpec(id, project, route, preset, "http://x/", x, 0f, 1440, 900, label = label, group = group)

    private val lane = LaneKey("p", "/", "desktop")

    @Test
    fun the_live_frame_is_the_rightmost_plain_frame_of_the_lane() {
        val specs = listOf(spec("old", 0f), spec("live", 3000f), spec("mid", 1500f))
        assertEquals("live", FrameLane.live(specs, lane)?.id)
    }

    @Test
    fun a_variant_is_never_the_live_frame_even_when_it_sits_further_right() {
        val specs = listOf(spec("og", 0f), spec("v", 9000f, label = "A", group = "g"))
        assertEquals("og", FrameLane.live(specs, lane)?.id)
    }

    @Test
    fun frames_of_other_routes_presets_or_projects_are_other_lanes() {
        val specs =
            listOf(
                spec("about", 5000f, route = "/about"),
                spec("phone", 6000f, preset = "phone"),
                spec("other", 7000f, project = "q"),
                spec("home", 100f),
            )
        assertEquals("home", FrameLane.live(specs, lane)?.id)
        assertNull(FrameLane.live(specs, LaneKey("p", "/x", "desktop")))
    }

    @Test
    fun a_lane_of_only_variants_has_no_live_frame() {
        assertNull(FrameLane.live(listOf(spec("v", 0f, label = "A", group = "g")), lane))
    }

    @Test
    fun only_the_newest_group_of_the_lane_counts() {
        // Paint order is z order: the later round's frames come last, whatever their group ids look like.
        val specs =
            listOf(
                spec("old-a", 1500f, label = "A", group = "vg_zzz"),
                spec("old-b", 2000f, label = "B", group = "vg_zzz"),
                spec("og", 0f),
                spec("new-a", 2500f, label = "A", group = "vg_aaa"),
                spec("new-b", 3000f, label = "B", group = "vg_aaa"),
            )
        assertEquals(listOf("new-a", "new-b"), FrameLane.variants(specs, lane).map { it.id })
    }

    @Test
    fun variants_come_by_label_then_position() {
        val specs =
            listOf(
                spec("c", 100f, label = "C", group = "g"),
                spec("a", 300f, label = "A", group = "g"),
                spec("b", 200f, label = "B", group = "g"),
                spec("og", 0f),
                spec("elsewhere", 50f, route = "/x", label = "A", group = "h"),
            )
        assertEquals(listOf("a", "b", "c"), FrameLane.variants(specs, lane).map { it.id })
    }
}
