package dev.penwise.frames

import dev.penwise.ink.PageStyle
import dev.penwise.ink.ViewportState
import dev.penwise.ink.panBy
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** The lock and the camera: the first fit, a switch, a lost frame, a frame that moved, a screen that comes late. */
class LayerLockTest {
    private var specs: List<FrameSpec> = emptyList()
    private val focused = ArrayList<String?>()
    private val announced = ArrayList<String?>()
    private val camera =
        ViewportState().apply {
            setPage(PageStyle("blank", 0f, 0f, infinite = true))
            setViewSize(2880f, 1600f)
        }
    private val lock = LayerLock({ camera }, { specs }, { focused += it }) { announced += it }

    private fun frame(
        id: String,
        x: Float,
        y: Float = 0f,
        group: String? = null,
        label: String = "",
        height: Int = 4000,
    ) = FrameSpec(
        id,
        "p",
        "/",
        "desktop",
        "http://x/",
        x,
        y,
        1440,
        900,
        boxHeight = height,
        group = group,
        label = label,
    )

    private fun change(next: List<FrameSpec>) {
        specs = next
        lock.framesChanged()
    }

    @Test
    fun the_first_frames_lock_the_camera_on_the_opening_frame() {
        change(listOf(frame("og", 2000f), frame("a", 3600f, group = "g", label = "A")))
        assertEquals("og", lock.locked)
        assertTrue(camera.locked)
        assertEquals(2000f, camera.transform.toPageX(0f), EPS)
        assertEquals(2f, camera.transform.scale, EPS)
        assertEquals(listOf<String?>("og"), focused)
        assertEquals(listOf<String?>("og"), announced)
    }

    @Test
    fun locking_a_variant_moves_the_camera_to_it_and_makes_it_live() {
        change(listOf(frame("og", 2000f), frame("a", 3600f, group = "g", label = "A")))
        lock.lockTo("a")
        assertEquals("a", lock.locked)
        assertEquals(3600f, camera.transform.toPageX(0f), EPS)
        assertEquals(listOf<String?>("og", "a"), focused)
        assertEquals(listOf<String?>("og", "a"), announced)
    }

    @Test
    fun the_variants_keep_their_scroll_position_when_the_owner_switches() {
        change(listOf(frame("og", 2000f), frame("a", 3600f, group = "g", label = "A")))
        camera.panBy(0f, -1000f)
        val top = camera.transform.toPageY(0f)
        lock.lockTo("a")
        assertEquals(top, camera.transform.toPageY(0f), EPS)
    }

    @Test
    fun a_frame_that_is_not_known_yet_is_locked_when_it_arrives() {
        change(listOf(frame("og", 2000f)))
        lock.lockTo("later")
        assertEquals("og", lock.locked)
        change(listOf(frame("og", 2000f), frame("later", 9000f, 5000f)))
        assertEquals("later", lock.locked)
        assertEquals(9000f, camera.transform.toPageX(0f), EPS)
        assertEquals(5000f, camera.transform.toPageY(0f), EPS)
    }

    @Test
    fun when_the_locked_variant_is_deleted_the_lanes_live_frame_takes_over() {
        val og = frame("og", 2000f)
        val variant = frame("a", 3600f, group = "g", label = "A")
        change(listOf(og, variant))
        lock.lockTo("a")
        change(listOf(og))
        assertEquals("og", lock.locked)
        assertEquals(2000f, camera.transform.toPageX(0f), EPS)
    }

    @Test
    fun a_locked_frame_that_moved_is_followed() {
        change(listOf(frame("og", 2000f)))
        change(listOf(frame("og", 5000f)))
        assertEquals(5000f, camera.transform.toPageX(0f), EPS)
        assertEquals("og", lock.locked)
    }

    @Test
    fun a_locked_frame_that_only_changed_height_keeps_the_camera_and_the_new_limits() {
        change(listOf(frame("og", 2000f)))
        camera.panBy(0f, -6000f)
        change(listOf(frame("og", 2000f, height = 1000)))
        assertEquals(1000f, camera.transform.toPageY(1600f), EPS)
    }

    private companion object {
        const val EPS = 1e-2f
    }
}
