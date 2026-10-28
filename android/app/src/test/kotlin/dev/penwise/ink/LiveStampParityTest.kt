package dev.penwise.ink

import dev.penwise.core.InkPresets
import dev.penwise.core.InkStroke
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import kotlin.math.sin

/**
 * A stamp stroke on the front buffer is the committed one: the live geometry (fed sample by sample, as the pen
 * delivers them) and the dry item the scene renders after pen-up carry the same dabs, opacity and grain, so the
 * coverage the tablet rasterises is the same, and no outline is drawn underneath.
 */
class LiveStampParityTest {
    private val argb = 0xFF202020.toInt()
    private val count = 400

    private fun wave(): Pair<FloatArray, IntArray> {
        val samples = FloatArray(count * 5)
        val times = IntArray(count)
        for (i in 0 until count) {
            samples[i * 5] = 100f + 0.9f * i
            samples[i * 5 + 1] = 300f + 40f * sin(i * 0.05f)
            samples[i * 5 + 2] = 0.3f + 0.4f * (i / count.toFloat())
            samples[i * 5 + 3] = 0.5f
            times[i] = i * 2_075
        }
        return samples to times
    }

    private fun stampPresets() =
        InkPresets.names.indices.filter { index ->
            InkStroke(index, 1L, argb, 1f).use { it.snapshot().isStamps }
        }

    @Test
    fun an_outline_stroke_keeps_its_predicted_tail() {
        val outline = InkStroke(0, 1L, argb, 1f).use { it.snapshot() }
        assertTrue(StrokeFrame(outline, outline).drawsTail)
    }

    @Test
    fun the_stamp_presets_are_found() {
        assertTrue(stampPresets().size >= 5)
    }

    @Test
    fun live_stamp_strokes_rasterise_like_the_committed_ones() {
        val (samples, times) = wave()
        SceneHarness().use { harness ->
            for (preset in stampPresets()) {
                val name = InkPresets.names[preset]
                val live = InkStroke(preset, SEED, argb, 1f)
                for (i in 0 until count) {
                    live.push(samples.copyOfRange(i * 5, i * 5 + 5), intArrayOf(times[i]), 1)
                }
                val shown = live.snapshot()
                live.close()
                assertTrue("$name: stamps are drawn as stamps, with no outline under them", shown.outline.isEmpty())
                assertFalse("$name: one layer, no predicted tail", StrokeFrame(shown, shown).drawsTail)

                val id = harness.scene.newItemId()
                harness.scene.beginGesture()
                harness.scene.commit(harness.scene.addStroke(id, preset, argb, 1f, SEED, samples, times))
                harness.scene.endGesture()
                val dry =
                    harness
                        .render(ItemKey(id))
                        .parts
                        .filterIsInstance<RenderPart.Stamps>()
                        .single()

                assertEquals("$name: stroke opacity", dry.opacity, shown.opacity, 0f)
                assertEquals("$name: grain scale", dry.grainScale, shown.grainScale, 0f)
                assertEquals("$name: grain softness", dry.grainSoftness, shown.grainSoftness, 0f)
                assertEquals("$name: colour", dry.argb, shown.argb)
                assertTrue("$name: same dabs", dry.dabs.contentEquals(shown.dabs))
                assertCoverageEqual(name, dry, shown)
            }
        }
    }

    private fun assertCoverageEqual(
        name: String,
        dry: RenderPart.Stamps,
        shown: dev.penwise.core.InkGeometry,
    ) {
        val view = RasterView(1f, 0f, 0f)
        val dryDabs = StampRaster.pixelDabs(dry.dabs, view)
        val liveDabs = StampRaster.pixelDabs(shown.dabs, view)
        val dryGrain = GrainStyle(dry.grainScale, dry.grainSoftness).takeIf { dry.grainScale > 0f }
        val liveGrain = GrainStyle(shown.grainScale, shown.grainSoftness).takeIf { shown.grainScale > 0f }
        val dryHeights = dryGrain?.let { g -> HeightSource { x, y -> PaperNoise.height(x, y, g.scale) } }
        val liveHeights = liveGrain?.let { g -> HeightSource { x, y -> PaperNoise.height(x, y, g.scale) } }
        for (col in 90..1000 step 7) {
            for (row in 240..360 step 5) {
                val a = StampRaster.coverageAt(dryDabs, col, row, view, dryGrain, dryHeights)
                val b = StampRaster.coverageAt(liveDabs, col, row, view, liveGrain, liveHeights)
                assertEquals("$name at ($col, $row)", a, b, 1e-4f)
            }
        }
    }

    private companion object {
        const val SEED = 7L
    }
}
