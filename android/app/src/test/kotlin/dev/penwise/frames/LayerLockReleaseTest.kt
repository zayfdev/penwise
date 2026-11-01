package dev.penwise.frames

import dev.penwise.ink.PageStyle
import dev.penwise.ink.ViewportState
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** The lock's start and end: a screen that comes late, and freeing the camera however the layer lets go. */
class LayerLockReleaseTest {
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
    ) = FrameSpec(id, "p", "/", "desktop", "http://x/", x, 0f, 1440, 900, boxHeight = 4000)

    private fun change(next: List<FrameSpec>) {
        specs = next
        lock.framesChanged()
    }

    @Test
    fun a_screen_that_comes_late_gets_the_fit_when_the_camera_settles() {
        val late = ViewportState().apply { setPage(PageStyle("blank", 0f, 0f, infinite = true)) }
        val waiting = LayerLock({ late }, { specs }, { focused += it }) { announced += it }
        specs = listOf(frame("og", 2000f))
        waiting.framesChanged()
        assertEquals(emptyList<String?>(), focused)
        late.setViewSize(2880f, 1600f)
        waiting.settle()
        assertEquals(2000f, late.transform.toPageX(0f), EPS)
        assertEquals(listOf<String?>("og"), focused)
    }

    @Test
    fun releasing_frees_the_camera_even_when_the_layer_let_go_of_it_first() {
        var held: ViewportState? = camera
        val layer = LayerLock({ held }, { specs }, { focused += it }) { announced += it }
        specs = listOf(frame("og", 2000f))
        layer.framesChanged()
        assertTrue(camera.locked)
        held = null
        layer.release()
        assertFalse(camera.locked)
    }

    @Test
    fun a_lock_waiting_for_the_screen_asks_to_be_settled_again_until_it_lands() {
        val late = ViewportState().apply { setPage(PageStyle("blank", 0f, 0f, infinite = true)) }
        val waiting = LayerLock({ late }, { specs }, { focused += it }) { announced += it }
        specs = listOf(frame("og", 2000f))
        waiting.framesChanged()
        assertTrue(waiting.settle())
        assertTrue(waiting.settle())
        late.setViewSize(2880f, 1600f)
        assertFalse(waiting.settle())
        assertEquals(listOf<String?>("og"), focused)
    }

    @Test
    fun releasing_frees_the_camera_and_clears_the_lock() {
        change(listOf(frame("og", 2000f)))
        lock.release()
        assertNull(lock.locked)
        assertFalse(camera.locked)
        assertEquals(listOf<String?>("og", null), announced)
        change(listOf(frame("og", 2000f)))
        change(emptyList())
        assertNull(lock.locked)
        assertFalse(camera.locked)
    }

    private companion object {
        const val EPS = 1e-2f
    }
}
