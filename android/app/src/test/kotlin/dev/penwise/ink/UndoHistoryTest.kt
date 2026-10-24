package dev.penwise.ink

import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/** Undo history survives a restart: the same steps come back, in the same order, with their strokes. */
class UndoHistoryTest {
    private fun stroke(seed: Long) =
        SavedStroke(
            presetIndex = 2,
            argb = -0x1000000,
            sizeScale = 1.2f,
            seed = seed,
            samples = floatArrayOf(10f, 20f, 0.5f, 0f, 0f, 40f, 50f, 0.5f, 0f, 0f),
            times = intArrayOf(0, 900),
            count = 2,
        )

    private fun dir(name: String): File =
        File(System.getProperty("java.io.tmpdir"), "penwise-undo-$name-${System.nanoTime()}")

    @Test
    fun steps_survive_a_round_trip_through_the_file() {
        val root = dir("roundtrip")
        try {
            val added = HistoryStep.Added(stroke(1))
            val removed = HistoryStep.Removed(listOf(3 to stroke(2), 0 to stroke(3)))
            val replaced = HistoryStep.Replaced(listOf(stroke(4)), listOf(stroke(5), stroke(6)))
            val undo = listOf(added, removed, replaced)
            val redo = listOf(HistoryStep.Added(stroke(7)))
            runBlocking {
                UndoHistoryFile(root, "pg_a").save(undo, redo)
                val (loadedUndo, loadedRedo) = UndoHistoryFile(root, "pg_a").load()
                assertEquals(3, loadedUndo.size)
                assertEquals(1, loadedRedo.size)
                assertEquals(listOf(1L), (loadedUndo[0] as HistoryStep.Added).let { listOf(it.stroke.seed) })
                val back = loadedUndo[1] as HistoryStep.Removed
                assertEquals(listOf(3, 0), back.removed.map { it.first })
                assertEquals(listOf(2L, 3L), back.removed.map { it.second.seed })
                val swap = loadedUndo[2] as HistoryStep.Replaced
                assertEquals(listOf(4L), swap.removed.map { it.seed })
                assertEquals(listOf(5L, 6L), swap.added.map { it.seed })
            }
        } finally {
            root.deleteRecursively()
        }
    }

    @Test
    fun a_page_with_no_history_has_an_empty_or_absent_file() {
        val root = dir("empty")
        try {
            runBlocking {
                assertTrue(UndoHistoryFile(root, "pg_none").load().first.isEmpty())
                UndoHistoryFile(root, "pg_none").save(emptyList(), emptyList())
                assertTrue("nothing to store means no file", !File(root, "pages/pg_none.undo").isFile)
            }
        } finally {
            root.deleteRecursively()
        }
    }

    @Test
    fun a_corrupt_file_is_ignored_rather_than_crashing() {
        val root = dir("corrupt")
        try {
            File(root, "pages").mkdirs()
            File(root, "pages/pg_bad.undo").writeBytes(byteArrayOf(1, 2, 3, 4, 5))
            val (undo, redo) = runBlocking { UndoHistoryFile(root, "pg_bad").load() }
            assertTrue(undo.isEmpty())
            assertTrue(redo.isEmpty())
        } finally {
            root.deleteRecursively()
        }
    }

    @Test
    fun restoring_history_makes_undo_work_again() {
        val store = StrokeStore()
        store.add(stroke(1))
        store.add(stroke(2))
        val (undo, redo) = store.historySnapshot()
        assertEquals(2, undo.size)

        val reopened = StrokeStore()
        reopened.restoreHistory(emptyList(), emptyList())
        assertTrue("a fresh store has nothing to undo", !reopened.canUndo())
        reopened.add(stroke(1))
        reopened.add(stroke(2))
        reopened.restoreHistory(undo, redo)
        assertTrue(reopened.canUndo())
        reopened.undo()
        assertEquals(listOf(1L), reopened.snapshot().map { it.seed })
        reopened.undo()
        assertTrue(reopened.snapshot().isEmpty())
        assertTrue(reopened.canRedo())
    }

    @Test
    fun a_restore_with_history_replaces_the_old_stacks() {
        val store = StrokeStore()
        store.add(stroke(1))
        store.restoreHistory(emptyList(), emptyList())
        assertTrue(!store.canUndo())
    }
}
