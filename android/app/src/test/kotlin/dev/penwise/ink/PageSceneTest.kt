package dev.penwise.ink

import dev.penwise.core.Native
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** The ink tools through the real Rust core: what the pen does must show up in the render data. */
class PageSceneTest {
    private val cut = floatArrayOf(320f, 60f, 320f, 140f)

    @Test
    fun a_drawn_stroke_is_an_item_with_outline_render_data_and_reaches_the_outbox() {
        SceneHarness().use { h ->
            val before = Native.syncOutboxCounts(h.sync)[0]
            val key = h.line(100f)
            assertEquals(listOf(key), h.items())
            val item = h.render(key)
            assertTrue(item.parts.single() is RenderPart.Outline)
            assertTrue("bounds cover the stroke", item.bounds.left < 20f && item.bounds.right > 600f)
            assertEquals(before + 1, Native.syncOutboxCounts(h.sync)[0])
            assertTrue(h.scene.canUndo)
        }
    }

    @Test
    fun an_empty_edit_blob_is_not_committed() {
        SceneHarness().use { h ->
            assertFalse(h.scene.commit(h.scene.erase(floatArrayOf(300f, 300f), 10f, precise = true)))
            assertTrue(h.committed.isEmpty())
            assertFalse(h.scene.undo())
        }
    }

    @Test
    fun a_precise_erase_splits_a_long_line_into_two_items() {
        SceneHarness().use { h ->
            val line = h.line(100f)
            h.erase(cut, 12f)
            val pieces = h.items()
            assertEquals(2, pieces.size)
            assertFalse("the original is gone", line in pieces)
            val (left, right) = pieces.map { h.render(it).bounds }.sortedBy { it.left }
            assertTrue("left piece stops at the cut", left.right < 320f && left.left < 30f)
            assertTrue("right piece starts after the cut", right.left > 320f && right.right > 600f)
        }
    }

    @Test
    fun a_whole_stroke_erase_removes_the_stroke_it_touches_and_keeps_the_others() {
        SceneHarness().use { h ->
            val hit = h.line(100f)
            val kept = h.line(400f)
            h.erase(floatArrayOf(300f, 80f, 300f, 120f), 12f, precise = false)
            assertEquals(listOf(kept), h.items())
            assertFalse(hit in h.items())
        }
    }

    @Test
    fun undo_restores_an_erased_line_and_redo_cuts_it_again() {
        SceneHarness().use { h ->
            h.line(100f)
            val whole = h.items().map { h.scene.itemFloats(it.bytes).toList() }
            h.erase(cut, 12f)
            assertEquals(2, h.items().size)

            assertTrue(h.scene.undo())
            val restored = h.items().map { h.scene.itemFloats(it.bytes).toList() }
            assertEquals("one undo step restores the original", whole, restored)
            assertTrue(h.scene.canRedo)

            assertTrue(h.scene.redo())
            assertEquals(2, h.items().size)

            assertTrue(h.scene.undo())
            assertTrue(h.scene.undo())
            assertTrue("undo past the first stroke empties the page", h.items().isEmpty())
            assertFalse(h.scene.undo())
        }
    }

    @Test
    fun a_tap_hits_the_stroke_under_it() {
        SceneHarness().use { h ->
            val key = h.line(100f)
            assertEquals(listOf(key), ItemKey.list(h.scene.hitTest(300f, 100f, 6f)))
            assertTrue(h.scene.hitTest(300f, 500f, 6f).isEmpty())
        }
    }

    @Test
    fun a_closed_scene_refuses_calls() {
        val h = SceneHarness()
        h.close()
        val failure = runCatching { h.scene.renderOrder() }.exceptionOrNull()
        assertNotNull(failure)
        assertTrue(failure is IllegalStateException)
    }
}
