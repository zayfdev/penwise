package dev.penwise.frames

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** Which WebView work a scene update means: create, update, reload or destroy (C1 → the layer). */
class FrameDiffTest {
    @Test
    fun a_new_frame_is_opened() {
        val plan = FrameDiff.plan(emptyList(), listOf(spec("a")))
        assertEquals(listOf(spec("a")), plan.open)
        assertEmpty(plan)
    }

    @Test
    fun a_frame_that_is_gone_is_closed() {
        val plan = FrameDiff.plan(listOf(spec("a")), emptyList())
        assertEquals(listOf("a"), plan.close)
        assertEquals(emptyList<FrameSpec>(), plan.open)
    }

    @Test
    fun moving_a_frame_only_translates_it() {
        val plan = FrameDiff.plan(listOf(spec("a", x = 10f, y = 20f)), listOf(spec("a", x = 40f, y = 20f)))
        assertEquals(listOf(spec("a", x = 40f, y = 20f)), plan.move)
        assertTrue(plan.open.isEmpty() && plan.load.isEmpty() && plan.resize.isEmpty() && plan.close.isEmpty())
    }

    @Test
    fun a_preset_switch_re_lays_out_without_a_reload() {
        val plan = FrameDiff.plan(listOf(spec("a")), listOf(spec("a", cssW = 834, cssH = 1194)))
        assertEquals(1, plan.resize.size)
        assertTrue(plan.load.isEmpty())
    }

    @Test
    fun another_route_or_project_loads_again() {
        val route = FrameDiff.plan(listOf(spec("a")), listOf(spec("a", url = "http://127.0.0.1:17423/about")))
        assertEquals(1, route.load.size)
        val project = FrameDiff.plan(listOf(spec("a")), listOf(spec("a", url = "http://127.0.0.1:17424/")))
        assertEquals(1, project.load.size)
    }

    @Test
    fun the_same_list_asks_for_nothing() {
        assertTrue(FrameDiff.plan(listOf(spec("a"), spec("b")), listOf(spec("a"), spec("b"))).isEmpty)
    }

    @Test
    fun a_whole_switch_keeps_the_survivor_and_touches_only_the_rest() {
        val plan =
            FrameDiff.plan(
                listOf(spec("a"), spec("b", x = 5f)),
                listOf(spec("b", x = 9f), spec("c")),
            )
        assertEquals(listOf(spec("b", x = 9f)), plan.move)
        assertEquals(listOf("a"), plan.close)
        assertEquals(1, plan.open.size)
    }

    private fun assertEmpty(plan: FramePlan) {
        assertTrue(plan.load.isEmpty())
        assertTrue(plan.resize.isEmpty())
        assertTrue(plan.move.isEmpty())
        assertTrue(plan.close.isEmpty())
    }

    private fun spec(
        id: String,
        url: String = "http://127.0.0.1:17423/",
        x: Float = 0f,
        y: Float = 0f,
        cssW: Int = 390,
        cssH: Int = 844,
    ) = FrameSpec(
        id = id,
        project = "portfolio",
        route = "/",
        preset = "phone",
        url = url,
        x = x,
        y = y,
        cssWidth = cssW,
        cssHeight = cssH,
    )
}
