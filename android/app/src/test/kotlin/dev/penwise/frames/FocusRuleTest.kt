package dev.penwise.frames

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/** Which frame is live (contract §3): coverage, the 60 % threshold, hysteresis and explicit focus. */
class FocusRuleTest {
    private val w = 2560f
    private val h = 1600f

    private fun box(
        id: String,
        left: Float,
        width: Float,
        top: Float = 0f,
        height: Float = 1600f,
    ) = ScreenBox(id, left, top, left + width, top + height)

    @Test
    fun the_frame_covering_most_of_the_screen_wins() {
        val rule = FocusRule()
        // Two frames over the threshold; B shows more of the screen.
        val a = box("a", left = -1000f, width = 1700f, height = 800f)
        val b = box("b", left = 100f, width = 2400f)
        assertEquals("b", rule.evaluate(listOf(a, b), w, h))
    }

    @Test
    fun a_frame_narrower_than_sixty_percent_of_the_screen_is_never_live() {
        val rule = FocusRule()
        val narrow = box("a", left = 0f, width = 0.59f * w)
        assertNull(rule.evaluate(listOf(narrow), w, h))
        val wide = box("a", left = 0f, width = 0.61f * w)
        assertEquals("a", rule.evaluate(listOf(wide), w, h))
    }

    @Test
    fun the_live_frame_does_not_flap_at_the_threshold() {
        val rule = FocusRule()
        assertEquals("a", rule.evaluate(listOf(box("a", 0f, 0.62f * w)), w, h))
        // Slightly under the enter threshold: the live frame stays (the exit threshold is lower).
        assertEquals("a", rule.evaluate(listOf(box("a", 0f, 0.55f * w)), w, h))
        // Clearly gone: no longer live.
        assertNull(rule.evaluate(listOf(box("a", 0f, 0.4f * w)), w, h))
        // And it must be wide again to come back, not just over the exit threshold.
        assertNull(rule.evaluate(listOf(box("a", 0f, 0.55f * w)), w, h))
    }

    @Test
    fun a_challenger_needs_clearly_more_screen_to_take_over() {
        val rule = FocusRule()
        val a = box("a", left = 0f, width = 1700f)
        assertEquals("a", rule.evaluate(listOf(a), w, h))
        // B covers about 13 % more than A: not enough.
        val a2 = box("a", left = -500f, width = 1700f)
        val b = box("b", left = 1200f, width = 1700f)
        assertEquals("a", rule.evaluate(listOf(a2, b), w, h))
        // B covers far more.
        val a3 = box("a", left = -1500f, width = 1700f)
        val b3 = box("b", left = 200f, width = 1700f)
        assertEquals("b", rule.evaluate(listOf(a3, b3), w, h))
    }

    @Test
    fun an_explicit_focus_wins_over_the_rule_while_the_frame_stays_on_screen() {
        val rule = FocusRule()
        rule.explicit("a")
        val a = box("a", left = -1400f, width = 1700f)
        val b = box("b", left = 300f, width = 2400f)
        assertEquals("a", rule.evaluate(listOf(a, b), w, h))
        // The frame really left: the rule decides again.
        val gone = box("a", left = -3000f, width = 1700f)
        assertEquals("b", rule.evaluate(listOf(gone, b), w, h))
    }

    @Test
    fun clearing_the_rule_leaves_nothing_live() {
        val rule = FocusRule()
        rule.explicit("a")
        rule.clear()
        assertNull(rule.focused)
    }
}
