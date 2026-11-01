package dev.penwise.overview

import dev.penwise.frames.LaneKey
import org.junit.Assert.assertEquals
import org.junit.Test

/** The now column's round: the project's open round on the server, else one after the newest resolved one. */
class OverviewRoundsTest {
    private val desktop = LaneKey("p", "/", "desktop")
    private val specs = listOf(frame("live", 3000f), frame("old", 0f))

    private fun now(
        history: Map<LaneKey, List<Generation>> = emptyMap(),
        rounds: Map<String, Int> = emptyMap(),
    ) = OverviewStrips
        .of(specs, history, emptyMap(), rounds) { null }
        .single()
        .columns
        .filterIsInstance<OverviewColumn.Now>()
        .single()

    @Test
    fun a_lane_without_generations_shows_the_projects_open_round() {
        assertEquals(5, now(rounds = mapOf("p" to 5)).round)
        assertEquals(5, now(mapOf(desktop to listOf(generation(2))), mapOf("p" to 5)).round)
    }

    @Test
    fun another_projects_round_is_not_used() {
        assertEquals(1, now(rounds = mapOf("q" to 9)).round)
    }

    @Test
    fun without_the_open_round_the_now_round_is_one_after_the_newest_resolved_one() {
        assertEquals(1, now().round)
        assertEquals(8, now(mapOf(desktop to listOf(generation(7), generation(4)))).round)
    }
}
