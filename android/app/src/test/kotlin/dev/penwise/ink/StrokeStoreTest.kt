package dev.penwise.ink

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** Undo/redo across add, clear and erase gestures. Geometry is not needed: the store keeps [SavedStroke]s. */
class StrokeStoreTest {
    private fun stroke(
        seed: Long,
        x: Float = 100f,
    ): SavedStroke =
        SavedStroke(
            presetIndex = 0,
            argb = -0x1000000,
            sizeScale = 1f,
            seed = seed,
            samples = floatArrayOf(x, 100f, 0.5f, 0f, 0f, x + 50f, 100f, 0.5f, 0f, 0f),
            times = intArrayOf(0, 1000),
            count = 2,
        )

    private fun sealed(vararg seeds: Long): StrokeStore = StrokeStore().apply { seeds.forEach { add(stroke(it)) } }

    @Test
    fun undo_and_redo_a_stroke() {
        val store = sealed(1, 2)
        assertTrue(store.canUndo())
        assertFalse(store.canRedo())
        store.undo()
        assertEquals(listOf(1L), store.snapshot().map { it.seed })
        assertTrue(store.canRedo())
        store.redo()
        assertEquals(listOf(1L, 2L), store.snapshot().map { it.seed })
    }

    @Test
    fun undo_after_clear_restores_order() {
        val store = sealed(1, 2, 3)
        store.clear()
        assertEquals(0, store.snapshot().size)
        store.undo()
        assertEquals(listOf(1L, 2L, 3L), store.snapshot().map { it.seed })
    }

    @Test
    fun a_new_stroke_clears_redo() {
        val store = sealed(1)
        store.undo()
        assertTrue(store.canRedo())
        store.add(stroke(2))
        assertFalse(store.canRedo())
    }

    @Test
    fun erase_is_undoable_and_restores_position() {
        val store = StrokeStore()
        store.add(stroke(1, x = 10f))
        store.add(stroke(2, x = 500f))
        store.add(stroke(3, x = 900f))
        val hit = store.eraseUnder(floatArrayOf(525f, 100f), 1, radius = 40f)
        assertTrue("the middle stroke is erased", hit)
        assertEquals(listOf(1L, 3L), store.snapshot().map { it.seed })
        store.commitErase()
        store.undo()
        assertEquals("order restored", listOf(1L, 2L, 3L), store.snapshot().map { it.seed })
    }

    @Test
    fun repeated_erase_hits_merge_into_one_step() {
        val store = StrokeStore()
        store.add(stroke(1, x = 10f))
        store.add(stroke(2, x = 500f))
        store.eraseUnder(floatArrayOf(20f, 100f), 1, radius = 40f, persist = false)
        store.eraseUnder(floatArrayOf(510f, 100f), 1, radius = 40f, persist = false)
        store.commitErase()
        assertEquals(0, store.snapshot().size)
        store.undo()
        assertEquals("one undo brings both back", 2, store.snapshot().size)
    }

    @Test
    fun erase_misses_leave_state_alone() {
        val store = sealed(1)
        assertFalse(store.eraseUnder(floatArrayOf(900f, 900f), 1, radius = 10f))
        assertFalse(store.canUndo() && store.snapshot().isEmpty())
    }

    @Test
    fun a_precise_erase_undoes_as_one_step() {
        val store = StrokeStore()
        val original = stroke(1)
        store.add(original)
        val left = stroke(2, x = 10f)
        val right = stroke(3, x = 80f)
        assertTrue(store.replace(listOf(1L), listOf(left, right)))
        assertEquals(listOf(2L, 3L), store.snapshot().map { it.seed })
        store.undo()
        assertEquals("one undo restores the whole stroke", listOf(1L), store.snapshot().map { it.seed })
        store.redo()
        assertEquals(listOf(2L, 3L), store.snapshot().map { it.seed })
    }

    @Test
    fun replacing_an_unknown_seed_changes_nothing() {
        val store = StrokeStore()
        store.add(stroke(1))
        assertTrue(!store.replace(listOf(99L), listOf(stroke(2))))
        assertEquals(listOf(1L), store.snapshot().map { it.seed })
        store.undo()
        assertTrue("only the add was recorded, so one undo empties the page", store.snapshot().isEmpty())
    }
}

/** Edits from other clients must land on screen without becoming this device's undo history. */
class RemoteEditTest {
    /** A stroke with geometry already built: `merge` only accepts strokes it could paint. */
    private fun stroke(seed: Long): SavedStroke =
        SavedStroke(
            0,
            -16_777_216,
            1f,
            seed,
            floatArrayOf(0f, 0f, 0.5f, 0f, 0f, 1f, 1f, 0.5f, 0f, 0f),
            intArrayOf(0, 8_000),
            2,
        ).apply {
            attachGeometry(
                dev.penwise.core.InkGeometry(
                    isStamps = false,
                    outline = floatArrayOf(0f, 0f, 1f, 1f),
                    contourEnds = intArrayOf(2),
                    dabs = FloatArray(0),
                    argb = -16_777_216,
                    opacity = 1f,
                    multiply = false,
                ),
            )
        }

    @Test
    fun a_remote_stroke_is_merged_and_cannot_be_undone_here() {
        val store = StrokeStore()
        assertTrue(store.merge(listOf(stroke(1L))))
        assertFalse("a remote stroke is not this device's step", store.canUndo())
    }

    @Test
    fun a_remote_removal_takes_the_stroke_away() {
        val store = StrokeStore()
        store.merge(listOf(stroke(1L), stroke(2L)))
        assertTrue(store.applyRemote(RemoteOp.Remove(setOf(1L))))
        assertFalse("nothing to remove", store.applyRemote(RemoteOp.Remove(setOf(99L))))
    }

    @Test
    fun a_remote_clear_empties_the_page_without_a_local_undo_step() {
        val store = StrokeStore()
        store.merge(listOf(stroke(1L), stroke(2L)))
        assertTrue(store.applyRemote(RemoteOp.Clear))
        assertFalse(store.canUndo())
    }
}

/** Legacy strokes arrived without a seed; every stroke must be individually addressable. */
class LegacySeedTest {
    private fun stroke(
        seed: Long,
        samples: FloatArray = floatArrayOf(0f, 0f, 0.5f, 0f, 0f),
    ): SavedStroke =
        SavedStroke(0, -16_777_216, 1f, seed, samples, intArrayOf(0), 1).apply {
            attachGeometry(
                dev.penwise.core.InkGeometry(
                    isStamps = false,
                    outline = floatArrayOf(0f, 0f, 1f, 1f),
                    contourEnds = intArrayOf(2),
                    dabs = FloatArray(0),
                    argb = -16_777_216,
                    opacity = 1f,
                    multiply = false,
                ),
            )
        }

    @Test
    fun strokes_without_a_seed_get_distinct_ones() {
        val store = StrokeStore()
        store.merge(listOf(stroke(0L), stroke(0L, floatArrayOf(5f, 5f, 0.5f, 0f, 0f))))
        val changed = store.assignMissingSeeds()
        assertEquals(2, changed.size)
        val seeds = store.snapshot().map { it.seed }
        assertEquals("seeds must differ: $seeds", 2, seeds.toSet().size)
        assertTrue(store.snapshot().none { it.seed == 0L })
    }

    @Test
    fun a_second_pass_finds_nothing_to_fix() {
        val store = StrokeStore()
        store.merge(listOf(stroke(7L)))
        assertTrue(store.assignMissingSeeds().isEmpty())
    }
}
