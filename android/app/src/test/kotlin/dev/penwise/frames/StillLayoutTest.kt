package dev.penwise.frames

import dev.penwise.ink.FrameSession
import dev.penwise.ink.SceneHarness
import dev.penwise.sync.Ids
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import org.junit.Assert.assertEquals
import org.junit.Test

/** Which layouts replace and which merge (contract §7), through the session that feeds the core. */
class StillLayoutTest {
    private val live =
        """{"preset":"desktop","scroll":[0,1200],"doc":[1440,6000],"viewport":[1440,900],"state":"","rects":{}}"""

    // A still without a scroll: a merge would keep the live scroll of 1200, a replacement forgets it.
    private val still = """{"preset":"desktop","doc":[1440,3000]}"""

    private fun windowY(kind: LayoutKind): Float {
        val job = Job()
        try {
            SceneHarness().use { harness ->
                val scene = harness.scene
                scene.addFrame("""{"project":"p","route":"/","preset":"desktop","x":0,"y":100,"extent":"full"}""")
                val id = SceneFrames.parse(scene.frames()).single().id
                val bytes = requireNotNull(Ids.bytes(id))
                val session =
                    FrameSession(CoroutineScope(job + Dispatchers.Unconfined), Dispatchers.Unconfined, { scene }, {})
                session.setLayout(id, live, LayoutKind.LIVE)
                assertEquals(1300f, requireNotNull(scene.frameLiveWindow(bytes))[1], 1e-3f)
                session.setLayout(id, still, kind)
                return requireNotNull(scene.frameLiveWindow(bytes))[1]
            }
        } finally {
            job.cancel()
        }
    }

    @Test
    fun a_new_still_for_a_frame_that_is_not_live_replaces_its_layout() {
        assertEquals(100f, windowY(LayoutKind.STILL), 1e-3f)
    }

    @Test
    fun a_demoted_frames_still_replaces_the_live_layout() {
        assertEquals(100f, windowY(LayoutKind.DEMOTED), 1e-3f)
    }

    @Test
    fun only_the_live_pages_reports_merge() {
        assertEquals(1300f, windowY(LayoutKind.LIVE), 1e-3f)
    }
}
