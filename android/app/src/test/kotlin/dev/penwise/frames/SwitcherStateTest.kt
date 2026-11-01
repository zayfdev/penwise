package dev.penwise.frames

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** The switcher: OG then variants by label, shown item, check on the pick, "Use this" rules, edge-swipe neighbours. */
class SwitcherStateTest {
    private fun frame(
        id: String,
        x: Float,
        label: String = "",
        group: String? = null,
        chosen: Boolean = false,
        route: String = "/",
    ) = FrameSpec(
        id,
        "p",
        route,
        "desktop",
        "http://x/",
        x,
        0f,
        1440,
        900,
        label = label,
        group = group,
        chosen = chosen,
    )

    private val lane =
        listOf(
            frame("og", 0f),
            frame("c", 3000f, "C", "g"),
            frame("a", 1500f, "A", "g", chosen = true),
            frame("b", 2200f, "B", "g"),
        )

    @Test
    fun the_row_is_og_then_the_variants_by_label() {
        val state = SwitcherState.of(lane, "og") ?: error("no switcher")
        assertEquals(listOf("OG", "A", "B", "C"), state.items.map { it.label })
        assertEquals(listOf("og", "a", "b", "c"), state.items.map { it.id })
    }

    @Test
    fun only_the_chosen_variant_has_a_check() {
        val state = SwitcherState.of(lane, "og") ?: error("no switcher")
        assertEquals(listOf(false, true, false, false), state.items.map { it.chosen })
    }

    @Test
    fun the_shown_item_is_the_locked_frame_from_any_member_of_the_lane() {
        assertEquals("b", SwitcherState.of(lane, "b")?.shown)
        assertEquals(SwitcherState.of(lane, "og")?.items, SwitcherState.of(lane, "c")?.items)
    }

    @Test
    fun the_row_lists_only_the_newest_group() {
        val two =
            listOf(frame("old", 900f, "A", "g1")) + lane.map { if (it.group == "g") it.copy(group = "g2") else it }
        val state = SwitcherState.of(two, "og") ?: error("no switcher")
        assertEquals(listOf("og", "a", "b", "c"), state.items.map { it.id })
    }

    @Test
    fun a_lane_without_variants_has_no_switcher() {
        assertNull(SwitcherState.of(listOf(frame("og", 0f), frame("other", 100f, route = "/x")), "og"))
        assertNull(SwitcherState.of(lane, null))
        assertNull(SwitcherState.of(lane, "missing"))
    }

    @Test
    fun use_this_is_hidden_on_og_and_on_the_variant_that_is_already_picked() {
        assertFalse(SwitcherState.of(lane, "og")?.canUseThis ?: true)
        assertFalse(SwitcherState.of(lane, "a")?.canUseThis ?: true)
        assertTrue(SwitcherState.of(lane, "b")?.canUseThis ?: false)
    }

    @Test
    fun the_neighbours_follow_the_row_and_stop_at_its_ends() {
        val state = SwitcherState.of(lane, "b") ?: error("no switcher")
        assertEquals("a", state.neighbour(-1))
        assertEquals("c", state.neighbour(1))
        assertNull(SwitcherState.of(lane, "og")?.neighbour(-1))
        assertNull(SwitcherState.of(lane, "c")?.neighbour(1))
        assertEquals("a", SwitcherState.of(lane, "og")?.neighbour(1))
    }

    @Test
    fun a_lane_whose_og_is_gone_still_lists_its_variants() {
        val state = SwitcherState.of(lane.filter { it.id != "og" }, "a") ?: error("no switcher")
        assertEquals(listOf("A", "B", "C"), state.items.map { it.label })
        assertNotNull(state.neighbour(1))
    }

    @Test
    fun a_plain_frame_that_is_not_the_lane_live_one_has_no_neighbours() {
        val state = SwitcherState.of(lane + frame("older", -500f), "older") ?: error("no switcher")
        assertNull(state.neighbour(1))
        assertFalse(state.canUseThis)
    }

    @Test
    fun a_read_only_row_never_offers_use_this() {
        val items =
            listOf(
                SwitcherItem("og", "OG", og = true, chosen = false),
                SwitcherItem("a", "A", og = false, chosen = false),
            )
        assertTrue(SwitcherState(items, "a").canUseThis)
        assertFalse(SwitcherState(items, "a", pickable = false).canUseThis)
    }
}
