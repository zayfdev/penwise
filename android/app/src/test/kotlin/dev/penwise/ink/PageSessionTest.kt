package dev.penwise.ink

import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/** Page I/O: ordered, atomic, and never pointed at another page's file before the swap. */
class PageSessionTest {
    private val root = File(System.getProperty("java.io.tmpdir"), "penwise-session-${System.nanoTime()}")

    @After
    fun cleanUp() {
        root.deleteRecursively()
    }

    private fun stroke(
        seed: Long,
        preset: Int = 0,
    ) = SavedStroke(preset, -0x1000000, 1f, seed, FloatArray(10) { it + seed.toFloat() }, intArrayOf(0, 7), 2)

    @Test
    fun rewrite_then_append_keeps_both() =
        runBlocking {
            val file = StrokeFile(root, "pg_a")
            file.append(stroke(1))
            file.rewrite(listOf(stroke(2), stroke(3)))
            file.append(stroke(4))
            assertEquals(listOf(2L, 3L, 4L), file.load().map { it.seed })
        }

    @Test
    fun queued_writes_land_in_the_order_they_were_requested() =
        runBlocking {
            val file = StrokeFile(root, "pg_a")
            val jobs =
                listOf(
                    launch { file.append(stroke(1)) },
                    launch { file.append(stroke(2)) },
                    launch { file.rewrite(listOf(stroke(9))) },
                    launch { file.append(stroke(10)) },
                )
            jobs.forEach { it.join() }
            assertEquals(listOf(9L, 10L), file.load().map { it.seed })
        }

    @Test
    fun a_rewrite_leaves_no_staging_file_and_replaces_the_old_content() =
        runBlocking {
            val file = StrokeFile(root, "pg_a")
            file.rewrite(listOf(stroke(1)))
            file.rewrite(listOf(stroke(2)))
            assertEquals(listOf(2L), file.load().map { it.seed })
            assertTrue(File(root, "pages").list().orEmpty().none { it.endsWith(".tmp") })
        }

    @Test
    fun undo_history_rewrites_atomically_too() =
        runBlocking {
            val undo = UndoHistoryFile(root, "pg_a")
            undo.save(listOf(HistoryStep.Added(stroke(1))), emptyList())
            undo.save(listOf(HistoryStep.Added(stroke(2))), emptyList())
            val (steps, _) = undo.load()
            assertEquals(listOf(2L), steps.map { (it as HistoryStep.Added).stroke.seed })
            assertTrue(File(root, "pages").list().orEmpty().none { it.endsWith(".tmp") })
        }

    @Test
    fun loading_another_page_leaves_the_open_page_files_alone_until_adopted() =
        runBlocking {
            StrokeFile(root, "pg_b").rewrite(listOf(stroke(5)))
            val session = PageSession(root, "pg_a")
            val before = session.file
            val beforeUndo = session.undoFile
            val loaded = session.load("pg_b", build = { })
            assertEquals("pg_a", session.id)
            assertSame(before, session.file)
            assertSame(beforeUndo, session.undoFile)
            session.adopt(loaded)
            assertEquals("pg_b", session.id)
            assertSame(loaded.file, session.file)
            assertSame(loaded.undoFile, session.undoFile)
        }

    @Test
    fun a_stroke_the_engine_refuses_is_skipped_on_load() =
        runBlocking {
            StrokeFile(root, "pg_a").rewrite(listOf(stroke(1), stroke(2, preset = 99), stroke(3)))
            val loaded =
                PageSession(root, "pg_a").load(
                    "pg_a",
                    build = { if (it.presetIndex == 99) error("preset out of range") },
                    onSkip = { _, _ -> },
                )
            assertEquals(listOf(1L, 3L), loaded.strokes.map { it.seed })
        }
}
