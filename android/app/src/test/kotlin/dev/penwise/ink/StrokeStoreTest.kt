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
}
