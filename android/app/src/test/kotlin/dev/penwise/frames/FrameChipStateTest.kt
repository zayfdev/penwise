package dev.penwise.frames

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The controls a frame's chip row shows (P6): the Ghost toggle only for a patched frame, the label and "Use this" only
 * for a variant, and every unpicked variant dimmed.
 */
class FrameChipStateTest {
    private fun spec(
        patch: String? = null,
        label: String = "",
        group: String? = null,
        chosen: Boolean = false,
    ) = FrameSpec(
        id = "00112233445566778899aabbccddeeff",
        project = "portfolio",
        route = "/",
        preset = "phone",
        url = "http://127.0.0.1:17421/",
        x = 0f,
        y = 0f,
        cssWidth = 390,
        cssHeight = 844,
        patch = patch,
        label = label,
        group = group,
        chosen = chosen,
    )

    private val patch = """{"css":"h1{color:red}"}"""

    @Test
    fun a_plain_frame_has_only_its_preset_route_and_reload() {
        val state = FrameChipState.of(spec(), ghostOn = true)
        assertFalse(state.ghost)
        assertFalse(state.useThis)
        assertFalse(state.chosen)
        assertFalse(state.dimmed)
        assertEquals("", state.label)
    }

    @Test
    fun a_patched_frame_shows_the_ghost_chip_in_its_toggle_state() {
        assertTrue(FrameChipState.of(spec(patch = patch), ghostOn = true).let { it.ghost && it.ghostOn })
        assertTrue(FrameChipState.of(spec(patch = patch), ghostOn = false).let { it.ghost && !it.ghostOn })
    }

    @Test
    fun a_frame_without_a_patch_never_shows_the_ghost_chip() {
        val state = FrameChipState.of(spec(), ghostOn = true)
        assertFalse(state.ghost)
        assertFalse(state.ghostOn)
    }

    @Test
    fun an_unpicked_variant_shows_its_label_use_this_and_is_dimmed() {
        val state = FrameChipState.of(spec(label = "Bold hero", group = "grp_1"), ghostOn = true)
        assertEquals("Bold hero", state.label)
        assertTrue(state.useThis)
        assertFalse(state.chosen)
        assertTrue(state.dimmed)
    }

    @Test
    fun the_chosen_variant_is_marked_and_not_dimmed() {
        val state = FrameChipState.of(spec(label = "Bold hero", group = "grp_1", chosen = true), ghostOn = true)
        assertTrue(state.chosen)
        assertFalse(state.useThis)
        assertFalse(state.dimmed)
    }

    @Test
    fun a_labelled_frame_outside_a_group_shows_no_variant_controls() {
        val state = FrameChipState.of(spec(label = "Base", chosen = true), ghostOn = true)
        assertEquals("", state.label)
        assertFalse(state.useThis)
        assertFalse(state.dimmed)
    }

    @Test
    fun a_variant_carries_its_ghost_and_its_choice_together() {
        val state =
            FrameChipState.of(spec(patch = patch, label = "Calm", group = "grp_1"), ghostOn = false)
        assertTrue(state.ghost)
        assertFalse(state.ghostOn)
        assertEquals("Calm", state.label)
        assertTrue(state.useThis)
    }
}
