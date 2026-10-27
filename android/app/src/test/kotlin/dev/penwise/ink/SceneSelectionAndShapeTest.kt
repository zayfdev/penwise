package dev.penwise.ink

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** Shapes, lasso selection and bulk edits through the real Rust core. */
class SceneSelectionAndShapeTest {
    private val rect = floatArrayOf(100f, 100f, 300f, 200f)

    @Test
    fun a_shape_is_created_from_a_drag_and_resized_by_a_handle() {
        SceneHarness().use { h ->
            val key = h.shape(ShapeMode.Rect, rect)
            assertEquals(listOf(key), h.items())
            val box = h.render(key).bounds
            assertTrue(box.left in 90f..110f && box.right in 290f..315f)
            assertTrue(box.top in 90f..110f && box.bottom in 190f..215f)

            val bottomRight = 4
            assertTrue(h.scene.commit(h.scene.resizeShape(key.bytes, bottomRight, 500f, 400f)))
            val grown = h.render(key).bounds
            assertTrue("resize moved the corner", grown.right > 490f && grown.bottom > 390f)
            assertEquals("the opposite corner stays", box.left, grown.left, 1f)
        }
    }

    @Test
    fun a_pencil_shape_is_drawn_with_an_outline_brush() {
        SceneHarness().use { h ->
            val pencil = 6
            val key = h.shape(ShapeMode.Ellipse, floatArrayOf(50f, 50f, 250f, 150f), pencil)
            assertTrue(h.render(key).parts.all { it is RenderPart.Outline })
        }
    }

    @Test
    fun erasing_a_shape_adds_a_mask_to_its_render_data() {
        SceneHarness().use { h ->
            val key = h.shape(ShapeMode.Rect, rect)
            assertTrue(h.render(key).masks.isEmpty())
            h.erase(floatArrayOf(200f, 80f, 200f, 220f), 8f)
            assertEquals(listOf(key), h.items())
            assertEquals(1, h.render(key).masks.size)
        }
    }

    @Test
    fun a_lasso_selects_what_it_encloses_and_a_move_shifts_exactly_those_items() {
        SceneHarness().use { h ->
            val inside = h.line(100f, 20f, 220f)
            val outside = h.line(900f, 20f, 220f)
            val loop = floatArrayOf(0f, 50f, 300f, 50f, 300f, 150f, 0f, 150f)
            assertEquals(listOf(inside), ItemKey.list(h.scene.lasso(loop)))

            val before = h.render(inside).bounds
            val untouched = h.render(outside).bounds
            h.scene.beginGesture()
            assertTrue(h.scene.commit(h.scene.moveItems(h.scene.lasso(loop), 60f, 25f)))
            h.scene.endGesture()

            val after = h.render(inside).bounds
            assertEquals(before.left + 60f, after.left, 1f)
            assertEquals(before.top + 25f, after.top, 1f)
            assertEquals(untouched.left, h.render(outside).bounds.left, 0f)

            assertTrue(h.scene.undo())
            assertEquals("undo puts it back", before.left, h.render(inside).bounds.left, 1f)
        }
    }

    @Test
    fun deleting_a_selection_and_clearing_the_page_are_single_undo_steps() {
        SceneHarness().use { h ->
            val first = h.line(100f)
            h.line(300f)
            h.scene.beginGesture()
            h.scene.commit(h.scene.deleteItems(first.bytes))
            h.scene.endGesture()
            assertEquals(1, h.items().size)
            assertTrue(h.scene.undo())
            assertEquals(2, h.items().size)

            assertTrue(h.scene.clearPage())
            assertTrue(h.items().isEmpty())
            assertTrue(h.scene.undo())
            assertEquals("one undo restores the whole page", 2, h.items().size)
        }
    }
}
