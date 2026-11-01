package dev.penwise.frames

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The controls a frame's chip row shows (P6): the Ghost toggle only for a patched frame.
 */
class FrameChipStateTest {
    private fun spec(
        patch: String? = null,
        label: String = "",
        group: String? = null,
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
    )

    private val patch = """{"css":"h1{color:red}"}"""

    @Test
    fun a_plain_frame_has_only_its_preset_route_and_reload() {
        val state = FrameChipState.of(spec(), ghostOn = true)
        assertFalse(state.ghost)
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
    fun a_variant_shows_only_its_ghost_the_label_and_pick_are_the_switchers() {
        val state =
            FrameChipState.of(spec(patch = patch, label = "Calm", group = "grp_1"), ghostOn = false)
        assertTrue(state.ghost)
        assertFalse(state.ghostOn)
    }
}
