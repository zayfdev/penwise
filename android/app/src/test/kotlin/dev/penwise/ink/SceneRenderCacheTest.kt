package dev.penwise.ink

import org.junit.Assert.assertEquals
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test

/** The render cache turns the native dirty list into the smallest repaint: draw only what changed. */
class SceneRenderCacheTest {
    private inline fun <reified T : RenderPlan> SceneRenderCache.expect(): T {
        val plan = update()
        assertTrue("expected ${T::class.simpleName}, was $plan", plan is T)
        return plan as T
    }

    @Test
    fun the_first_update_paints_everything_then_the_cache_is_idle() {
        SceneHarness().use { h ->
            h.line(100f)
            val cache = SceneRenderCache(h.scene)
            cache.expect<RenderPlan.Full>()
            assertEquals(1, cache.paintOrder.size)
            assertSame(RenderPlan.Idle, cache.update())
        }
    }

    @Test
    fun a_new_stroke_is_appended_without_repainting_the_page() {
        SceneHarness().use { h ->
            h.line(100f)
            val cache = SceneRenderCache(h.scene)
            cache.update()
            val fresh = h.line(300f)
            val plan = cache.expect<RenderPlan.Append>()
            assertEquals(1, plan.items.size)
            assertSame(cache[fresh], plan.items.single())
            assertSame(RenderPlan.Idle, cache.update())
        }
    }

    @Test
    fun an_erase_repaints_only_the_area_of_the_items_it_changed() {
        SceneHarness().use { h ->
            h.line(100f)
            h.line(900f)
            val cache = SceneRenderCache(h.scene)
            cache.update()
            h.scene.commit(h.scene.erase(floatArrayOf(320f, 60f, 320f, 140f), 12f, precise = true))
            val area = cache.expect<RenderPlan.Region>().area
            assertTrue("the area covers the cut line", area.top < 100f && area.bottom > 100f)
            assertTrue("and not the untouched one", area.bottom < 500f)
            assertEquals(3, cache.paintOrder.size)
        }
    }

    @Test
    fun undo_repaints_the_removed_stroke_area_and_drops_it_from_the_order() {
        SceneHarness().use { h ->
            h.line(100f)
            val cache = SceneRenderCache(h.scene)
            cache.update()
            h.scene.undo()
            val area = cache.expect<RenderPlan.Region>().area
            assertTrue(area.top < 100f && area.bottom > 100f)
            assertTrue(cache.paintOrder.isEmpty())
        }
    }

    @Test
    fun a_reload_repaints_everything() {
        SceneHarness().use { h ->
            h.line(100f)
            val cache = SceneRenderCache(h.scene)
            cache.update()
            h.scene.reload()
            cache.expect<RenderPlan.Full>()
            assertEquals(1, cache.paintOrder.size)
        }
    }

    @Test
    fun a_new_cache_on_a_scene_the_old_cache_already_drained_still_shows_the_page() {
        SceneHarness().use { h ->
            val kept = h.line(100f)
            SceneRenderCache(h.scene).update()
            val second = SceneRenderCache(h.scene)
            second.update()
            assertEquals(listOf(kept), second.paintOrder)
            assertTrue(second[kept] != null)
        }
    }
}
