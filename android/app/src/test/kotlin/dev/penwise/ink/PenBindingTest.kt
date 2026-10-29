package dev.penwise.ink

import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/** The M2 commit decision: bound with an answered element, area without one or when pinned, none off every frame. */
class PenBindingTest {
    private val frame = "00112233445566778899aabbccddeeff"
    private val key = "v1||h1#hero.hero-title"

    @Test
    fun an_answered_element_makes_a_bound_stroke() {
        assertEquals(BindMode.BOUND, BindDecision.mode(onFrame = true, bindToElement = true, key = key))
    }

    @Test
    fun no_answer_by_pen_up_falls_back_to_the_area() {
        assertEquals(BindMode.AREA, BindDecision.mode(onFrame = true, bindToElement = true, key = null))
        assertEquals(BindMode.AREA, BindDecision.mode(onFrame = true, bindToElement = true, key = ""))
    }

    @Test
    fun pin_to_area_ignores_the_element() {
        assertEquals(BindMode.AREA, BindDecision.mode(onFrame = true, bindToElement = false, key = key))
    }

    @Test
    fun a_pen_down_off_every_frame_has_no_binding() {
        assertNull(BindDecision.mode(onFrame = false, bindToElement = true, key = key))
        assertNull(BindDecision.mode(onFrame = false, bindToElement = false, key = null))
    }

    @Test
    fun binding_json_carries_the_frame_mode_and_key() {
        val binding =
            PendingBinding(
                frame = frame,
                bindToElement = true,
                answer = { HitAnswer(key, listOf(1f, 2f, 30f, 40f)) },
            )
        val json = JSONObject(requireNotNull(binding.json()))
        assertEquals(frame, json.getString("frame"))
        assertEquals("bound", json.getString("mode"))
        assertEquals(key, json.getString("key"))
    }

    @Test
    fun an_unanswered_request_commits_an_area_binding() {
        val binding = PendingBinding(frame = frame, bindToElement = true, answer = { null })
        val json = JSONObject(requireNotNull(binding.json()))
        assertEquals("area", json.getString("mode"))
        assertEquals(false, json.has("key"))
    }

    @Test
    fun pin_to_area_commits_area_even_with_an_answer() {
        val binding = PendingBinding(frame = frame, bindToElement = false, answer = { HitAnswer(key, emptyList()) })
        assertEquals("area", JSONObject(requireNotNull(binding.json())).getString("mode"))
    }

    @Test
    fun the_layout_is_primed_only_for_an_answered_bound_stroke() {
        var primed = 0
        val answered =
            PendingBinding(
                frame,
                bindToElement = true,
                answer = { HitAnswer(key, listOf(1f, 2f, 3f, 4f)) },
                prime = { _, _ -> primed++ },
            )
        // No scene (the page was left) primes nothing.
        answered.prepare(null)
        assertEquals(0, primed)
        val pinned =
            PendingBinding(
                frame = frame,
                bindToElement = false,
                answer = { HitAnswer(key, listOf(1f, 2f, 3f, 4f)) },
            )
        pinned.prepare(null)
        assertEquals(0, primed)
    }

    @Test
    fun releasing_forgets_the_pending_hit() {
        var released = 0
        PendingBinding(frame = frame, bindToElement = true, answer = { null }, onDone = { released++ }).release()
        assertEquals(1, released)
    }
}
