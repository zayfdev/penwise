package dev.penwise.frames

import dev.penwise.ink.SceneHarness
import dev.penwise.sync.Ids
import org.json.JSONArray
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** The JNI exports of the core's frame space, driven through the real host library. */
class FrameSpaceNativeTest {
    private fun layout(scrollY: Float) =
        """{"preset":"desktop","scroll":[0,$scrollY],"doc":[1440,5000],"viewport":[1440,900],"state":"","rects":{}}"""

    @Test
    fun the_live_window_of_a_full_frame_follows_the_reported_scroll_and_points_convert_both_ways() {
        SceneHarness().use { harness ->
            val scene = harness.scene
            scene.addFrame("""{"project":"p","route":"/","preset":"desktop","x":100,"y":200,"extent":"full"}""")
            val frame = SceneFrames.parse(scene.frames()).single()
            assertTrue(frame.full)
            val id = requireNotNull(Ids.bytes(frame.id))
            scene.setFrameLayout(id, layout(scrollY = 1200f))
            assertArrayEquals(floatArrayOf(100f, 1400f, 1440f, 900f), requireNotNull(scene.frameLiveWindow(id)), 1e-3f)
            val doc = requireNotNull(scene.frameDocPoint(id, 150f, 1500f))
            assertArrayEquals(floatArrayOf(50f, 1300f), doc, 1e-3f)
            assertArrayEquals(
                floatArrayOf(150f, 1500f),
                requireNotNull(scene.frameBoardPoint(id, doc[0], doc[1])),
                1e-3f,
            )
            // The window moves when the scroll does; the document point of a board point does not.
            scene.setFrameLayout(id, layout(scrollY = 2000f))
            assertEquals(2200f, requireNotNull(scene.frameLiveWindow(id))[1], 1e-3f)
            assertArrayEquals(doc, requireNotNull(scene.frameDocPoint(id, 150f, 1500f)), 1e-3f)
        }
    }

    @Test
    fun a_demoted_frames_still_layout_replaces_the_live_one_in_one_call() {
        SceneHarness().use { harness ->
            val scene = harness.scene
            scene.addFrame("""{"project":"p","route":"/","preset":"desktop","x":100,"y":200,"extent":"full"}""")
            val frame = SceneFrames.parse(scene.frames()).single()
            val id = requireNotNull(Ids.bytes(frame.id))
            assertEquals(frame.id, JSONArray(scene.frames()).getJSONObject(0).getString("hex"))
            scene.setFrameLayout(id, layout(scrollY = 1200f))
            assertEquals(1400f, requireNotNull(scene.frameLiveWindow(id))[1], 1e-3f)
            // One call: the still's layout (scrolled to the top, document 5000 tall) and nothing of the live one.
            assertTrue(scene.replaceFrameLayout(id, layout(scrollY = 0f)))
            assertEquals(200f, requireNotNull(scene.frameLiveWindow(id))[1], 1e-3f)
            assertEquals(5000, JSONArray(scene.frames()).getJSONObject(0).getInt("boxH"))
            // Unreadable input is refused and the layout stays.
            assertFalse(runCatching { scene.replaceFrameLayout(id, "nope") }.isSuccess)
            assertEquals(5000, JSONArray(scene.frames()).getJSONObject(0).getInt("boxH"))
        }
    }

    @Test
    fun a_viewport_frame_keeps_its_window_at_its_box() {
        SceneHarness().use { harness ->
            val scene = harness.scene
            scene.addFrame("""{"project":"p","route":"/","preset":"desktop","x":100,"y":200,"extent":"viewport"}""")
            val id = requireNotNull(Ids.bytes(SceneFrames.parse(scene.frames()).single().id))
            scene.setFrameLayout(id, layout(scrollY = 1200f))
            assertEquals(200f, requireNotNull(scene.frameLiveWindow(id))[1], 1e-3f)
        }
    }

    @Test
    fun an_unknown_frame_has_no_window_and_the_frame_json_carries_the_box() {
        SceneHarness().use { harness ->
            val unknown = ByteArray(16) { 9 }
            assertNull(harness.scene.frameLiveWindow(unknown))
            assertNull(harness.scene.frameDocPoint(unknown, 0f, 0f))
            harness.scene.addFrame("""{"project":"p","route":"/","preset":"desktop","x":0,"y":0}""")
            val json = JSONArray(harness.scene.frames()).getJSONObject(0)
            assertEquals("full", json.getString("extent"))
            assertEquals(900, json.getInt("boxH"))
        }
    }
}
