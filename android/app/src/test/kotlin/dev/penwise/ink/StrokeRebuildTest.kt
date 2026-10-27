package dev.penwise.ink

import org.junit.Assert.assertEquals
import org.junit.Test

/** A stroke the engine refuses to build (e.g. an out-of-range preset from the server) costs only itself. */
class StrokeRebuildTest {
    private fun stroke(
        seed: Long,
        preset: Int = 0,
    ) = SavedStroke(preset, -0x1000000, 1f, seed, FloatArray(10), intArrayOf(0, 1), 2)

    private val refusing: (SavedStroke) -> Unit = { if (it.presetIndex == BAD_PRESET) error("preset out of range") }

    @Test
    fun an_unbuildable_stroke_is_skipped_and_the_rest_survive() {
        val skipped = ArrayList<Long>()
        val kept =
            StrokeRebuild.usable(listOf(stroke(1), stroke(2, BAD_PRESET), stroke(3)), refusing) { s, _ ->
                skipped += s.seed
            }
        assertEquals(listOf(1L, 3L), kept.map { it.seed })
        assertEquals(listOf(2L), skipped)
    }

    @Test
    fun history_steps_with_an_unbuildable_stroke_are_dropped() {
        val steps =
            listOf(
                HistoryStep.Added(stroke(1)),
                HistoryStep.Removed(listOf(0 to stroke(2), 1 to stroke(3, BAD_PRESET))),
                HistoryStep.Replaced(listOf(stroke(4)), listOf(stroke(5))),
            )
        val kept = HistoryGeometry.usable(steps, refusing) { _, _ -> }
        assertEquals(listOf(steps[0], steps[2]), kept)
    }

    private companion object {
        const val BAD_PRESET = 99
    }
}
