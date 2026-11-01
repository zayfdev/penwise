package dev.penwise.overview

import dev.penwise.frames.LaneKey
import org.junit.Assert.assertEquals
import org.junit.Test

/** The overview opens on the strip the owner was last on, also when it is opened from a sketch page. */
class OverviewLaneTest {
    private val specs = listOf(frame("live", 0f))
    private val generation = generation(2, "A" to false, "B" to true)

    @Test
    fun the_overview_opens_on_the_lane_of_the_locked_frame() {
        val controller = OverviewController()
        val phone = frame("phone", 0f, y = 2000f, preset = "phone")
        controller.request("board", specs + phone, locked = "phone")
        assertEquals(LaneKey("p", "/", "phone"), controller.ui.value.lane)
    }

    @Test
    fun a_sketch_page_reopens_the_overview_on_the_strip_it_belongs_to() {
        val controller = OverviewController()
        val phone = LaneKey("p", "/", "phone")
        controller.request("board", specs, locked = "live")
        controller.enterSketch("sketch", phone)
        controller.request("sketch", emptyList())
        assertEquals(phone, controller.ui.value.lane)
    }

    @Test
    fun the_last_viewed_picture_keeps_its_lane_for_the_next_opening() {
        val controller = OverviewController()
        val phone = LaneKey("p", "/", "phone")
        controller.request("board", specs)
        controller.view(phone, generation, "item-2-og")
        controller.leaveViewer()
        assertEquals(phone, controller.ui.value.lane)
    }
}
