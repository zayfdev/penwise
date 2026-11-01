package dev.penwise.frames

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/** A board opens fitted to one frame: the top lane's frame, never an agent's variant. */
class FrameOpeningTest {
    private fun spec(
        id: String,
        x: Float,
        y: Float,
        group: String? = null,
    ) = FrameSpec(id, "p", "/", "desktop", "http://x/", x, y, 1440, 900, full = true, group = group)

    @Test
    fun the_top_lane_wins_over_a_lane_below() {
        assertEquals("home", FrameOpening.target(listOf(spec("about", 0f, 9000f), spec("home", 1500f, 0f))))
    }

    @Test
    fun a_variant_is_skipped_even_when_it_sits_higher_or_further_left() {
        val specs = listOf(spec("variant", 0f, -10f, group = "g"), spec("live", 1500f, 0f))
        assertEquals("live", FrameOpening.target(specs))
    }

    @Test
    fun a_board_of_only_variants_still_opens_on_one_and_an_empty_board_on_none() {
        assertEquals("a", FrameOpening.target(listOf(spec("b", 2000f, 0f, "g"), spec("a", 0f, 0f, "g"))))
        assertNull(FrameOpening.target(emptyList()))
    }
}
