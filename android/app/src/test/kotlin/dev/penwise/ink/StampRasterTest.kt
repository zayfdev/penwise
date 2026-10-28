package dev.penwise.ink

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * Parity with core: `crates/penwise-core/tests/goldens/stamp_coverage.txt` holds the coverage core's `render_stamps`
 * leaves at sample pixels (format in `tests/stamp_parity.rs`). The tablet's port of the formulas must reproduce it.
 */
class StampRasterTest {
    private class Sample(
        val col: Int,
        val row: Int,
        val height: Float,
        val s: Float,
    )

    private class Case(
        val name: String,
        val view: RasterView,
        val grain: GrainStyle?,
        val dabs: FloatArray,
        val samples: List<Sample>,
    )

    private class HeightRow(
        val x: Float,
        val y: Float,
        val scale: Float,
        val value: Float,
    )

    private class Fixture(
        val cases: List<Case>,
        val heights: List<HeightRow>,
    )

    private fun fixture(): Fixture {
        val file =
            generateSequence(File("").absoluteFile) { it.parentFile }
                .map { File(it, "crates/penwise-core/tests/goldens/stamp_coverage.txt") }
                .first { it.exists() }
        val cases = ArrayList<Case>()
        val heights = ArrayList<HeightRow>()
        var header: List<String>? = null
        val dabs = ArrayList<Float>()
        val samples = ArrayList<Sample>()

        fun flush() {
            val h = header ?: return
            val grainScale = h[5].toFloat()
            cases +=
                Case(
                    h[1],
                    RasterView(h[2].toFloat(), h[3].toFloat(), h[4].toFloat()),
                    if (grainScale > 0f) GrainStyle(grainScale, h[6].toFloat()) else null,
                    dabs.toFloatArray(),
                    samples.toList(),
                )
            dabs.clear()
            samples.clear()
        }
        for (line in file.readLines()) {
            val t = line.trim().split(Regex("\\s+"))
            when (t[0]) {
                "case" -> {
                    flush()
                    header = t
                }

                "dab" -> {
                    t.drop(1).forEach { dabs += it.toFloat() }
                }

                "at" -> {
                    samples += Sample(t[1].toInt(), t[2].toInt(), t[3].toFloat(), t[4].toFloat())
                }

                "height" -> {
                    heights += HeightRow(t[1].toFloat(), t[2].toFloat(), t[3].toFloat(), t[4].toFloat())
                }
            }
        }
        flush()
        return Fixture(cases, heights)
    }

    private val fix = fixture()

    @Test
    fun `fixture is loaded`() {
        assertTrue(fix.cases.size >= 5)
        assertTrue(fix.heights.size > 50)
    }

    @Test
    fun `paper noise is core's paper_height`() {
        var worst = 0f
        for (row in fix.heights) {
            worst = maxOf(worst, kotlin.math.abs(PaperNoise.height(row.x, row.y, row.scale) - row.value))
        }
        assertTrue("worst paper height error $worst", worst < 1e-4f)
    }

    @Test
    fun `per pixel coverage matches core with core's heights`() {
        for (c in fix.cases) {
            val dabs = StampRaster.pixelDabs(c.dabs, c.view)
            for (s in c.samples) {
                val got = StampRaster.coverageAt(dabs, s.col, s.row, c.view, c.grain) { _, _ -> s.height }
                assertEquals("${c.name} at (${s.col}, ${s.row})", s.s, got, 1e-4f)
            }
        }
    }

    @Test
    fun `the paper cache is sized to the pixels used, not to the reused coverage buffer`() {
        val fresh = StampRaster.paperScratch(null, 12)
        assertEquals(12, fresh.size)
        assertTrue(fresh.all { it.isNaN() })
        fresh.fill(1f)
        assertTrue("a big enough scratch is reused and reset", StampRaster.paperScratch(fresh, 8) === fresh)
        assertTrue(fresh.take(8).all { it.isNaN() })
        assertEquals(20, StampRaster.paperScratch(fresh, 20).size)
    }

    @Test
    fun `the buffer path matches core end to end with the ported noise`() {
        for (c in fix.cases) {
            val dabs = StampRaster.pixelDabs(c.dabs, c.view)
            val rect = StampRaster.rect(dabs, PixelRect(0, 0, SIDE, SIDE))
            val buffer = FloatArray(SIDE * SIDE)
            val heights = c.grain?.let { g -> HeightSource { x, y -> PaperNoise.height(x, y, g.scale) } }
            if (rect != null) StampRaster.accumulate(buffer, rect, dabs, c.view, c.grain, heights)
            val width = rect?.width ?: 0
            for (s in c.samples) {
                val got =
                    rect?.let { r ->
                        if ((s.col to s.row) in r) buffer[(s.row - r.top) * width + (s.col - r.left)] else 0f
                    } ?: 0f
                assertEquals("${c.name} at (${s.col}, ${s.row})", s.s, got, 5e-4f)
            }
        }
    }

    @Test
    fun `grain is applied per dab before the union`() {
        val dabs =
            floatArrayOf(
                20f,
                20f,
                10f,
                10f,
                0f,
                1f,
                1f,
                1f,
                0.3f,
                20f,
                20f,
                10f,
                10f,
                0f,
                1f,
                1f,
                1f,
                0.9f,
            )
        val view = RasterView()
        val pds = StampRaster.pixelDabs(dabs, view)
        val grain = GrainStyle(1f, 0.15f)
        // Paper height 0.5: the level-0.3 dab is blocked (edge 0.7), the level-0.9 dab passes (edge 0.1).
        val s = StampRaster.coverageAt(pds, 20, 20, view, grain) { _, _ -> 0.5f }
        val low = DabMath.grainCoverage(0.5f, 0.3f, 0.15f, 1f)
        val high = DabMath.grainCoverage(0.5f, 0.9f, 0.15f, 1f)
        val f = StampRaster.falloffAt(pds[0], 20, 20)
        assertEquals(f * low + f * high * (1f - f * low), s, 1e-4f)
        assertTrue("different dab levels give different coverage than one shared level", low < 0.05f && high > 0.95f)
    }

    @Test
    fun `a sub-pixel dab keeps core's minimum fade instead of vanishing`() {
        val d = floatArrayOf(10.5f, 10.5f, 0.3f, 0.3f, 0f, 1f, 0.5f, 0f, 1f)
        val pds = StampRaster.pixelDabs(d, RasterView())
        assertEquals(1, pds.size)
        assertTrue(StampRaster.coverageAt(pds, 10, 10, RasterView(), null, null) > 0.5f)
    }

    private companion object {
        const val SIDE = 96
    }
}
