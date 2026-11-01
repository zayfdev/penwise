package dev.penwise.frames

import dev.penwise.ink.PageRect
import dev.penwise.ink.PageStyle
import dev.penwise.ink.PenActivity
import dev.penwise.ink.ViewportState
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Camera moves other devices ask for: `Focus` locks a frame, `Focus` without an item opens the overview, a page without
 * frames fits its content, a drawing hand waits.
 */
class FrameAgentTest {
    private val job = Job()
    private val pen = PenActivity()
    private var now = 100_000L
    private var current = "page"
    private val opened = ArrayList<String>()
    private var frames: List<FrameSpec> = emptyList()
    private var bounds: PageRect? = null
    private var grids = 0
    private val control = FrameControl()
    private val viewport =
        ViewportState().apply {
            setPage(PageStyle("blank", 0f, 0f, infinite = true))
            setViewSize(2560f, 1600f)
        }
    private val agent =
        FrameAgent(
            scope = CoroutineScope(job),
            viewport = viewport,
            pen = pen,
            control = control,
            page =
                AgentPage(
                    current = { current },
                    open = { opened += it },
                    title = { "Title $it" },
                    specs = { frames },
                    contentBounds = { bounds },
                    openOverview = { grids++ },
                ),
            now = { now },
        )

    @After
    fun stop() = job.cancel()

    private fun frame(
        id: String,
        x: Float,
        y: Float,
        route: String = "/$id",
    ) = FrameSpec(id, "p", route, "desktop", "http://x/", x, y, 1440, 900, full = true, boxHeight = 4000)

    @Test
    fun a_focus_locks_the_frame_and_leaves_the_pages_alone() {
        frames = listOf(frame("a", 0f, 0f), frame("b", 2000f, 100f))
        agent.offer(MoveTarget("page", "b", ""))
        assertEquals("b", control.pendingLock)
        assertEquals(0, grids)
        assertTrue(opened.isEmpty())
    }

    @Test
    fun a_focus_without_an_item_opens_the_overview() {
        frames = listOf(frame("a", 0f, 0f))
        agent.offer(MoveTarget("page", null, ""))
        assertEquals(1, grids)
        assertNull(control.pendingLock)
    }

    @Test
    fun an_item_that_is_no_frame_changes_nothing_on_a_page_with_frames() {
        frames = listOf(frame("a", 0f, 0f))
        bounds = PageRect(-600f, -400f, 9_000f, 7_000f)
        val before = viewport.transform
        agent.offer(MoveTarget("page", "not-a-frame", ""))
        assertEquals(before, viewport.transform)
        assertEquals(0, grids)
        assertNull(control.pendingLock)
    }

    @Test
    fun an_item_that_is_no_frame_fits_all_the_content_of_a_page_without_frames() {
        bounds = PageRect(-600f, -400f, 9_000f, 7_000f)
        agent.offer(MoveTarget("page", "not-a-frame", ""))
        val view = viewport.transform
        assertTrue(view.toScreenX(-600f) >= -EPS && view.toScreenX(8_400f) <= 2560f + EPS)
        assertEquals("the height decides: 1600 / 7000", 1600f / 7_000f, view.scale, EPS)
    }

    @Test
    fun a_page_without_frames_or_content_resets_the_camera() {
        viewport.transformBy(2f, 0f, 0f, 100f, 100f)
        bounds = null
        agent.offer(MoveTarget("page", "gone", ""))
        assertEquals(1f, viewport.transform.scale, EPS)
        assertEquals(0f, viewport.transform.tx, EPS)
    }

    @Test
    fun while_the_owner_draws_nothing_moves_and_the_chip_offers_the_move() {
        frames = listOf(frame("a", 0f, 0f), frame("b", 2000f, 0f))
        pen.lastDrawAt = now - 4_000
        agent.offer(MoveTarget("page", "b", ""))
        val chip = agent.chip.value
        assertNotNull(chip)
        assertEquals("/b", chip?.label)
        assertNull(control.pendingLock)
        agent.accept()
        assertNull(agent.chip.value)
        assertEquals("b", control.pendingLock)
        // A held move can also be dismissed: it is gone and accepting does nothing.
        control.pendingLock = null
        pen.lastDrawAt = now - 1_000
        agent.offer(MoveTarget("page", "a", ""))
        agent.dismiss()
        assertNull(agent.chip.value)
        agent.accept()
        assertNull(control.pendingLock)
    }

    @Test
    fun a_move_ten_quiet_seconds_after_the_last_stroke_just_happens() {
        frames = listOf(frame("a", 0f, 0f), frame("b", 2000f, 0f))
        pen.lastDrawAt = now - 10_000
        agent.offer(MoveTarget("page", "b", ""))
        assertNull(agent.chip.value)
        assertEquals("b", control.pendingLock)
    }

    @Test
    fun a_move_to_another_page_locks_once_that_pages_frames_have_arrived() {
        frames = emptyList()
        agent.offer(MoveTarget("other", "b", ""))
        assertEquals(listOf("other"), opened)
        assertNull(control.pendingLock)
        // The new page opens and its first sweep finds the frame.
        current = "other"
        frames = listOf(frame("b", 2000f, 0f))
        agent.framesChanged()
        assertEquals("b", control.pendingLock)
    }

    @Test
    fun a_plain_page_switch_changes_the_page_and_leaves_the_camera_alone() {
        frames = listOf(frame("a", 0f, 0f))
        val before = viewport.transform
        agent.offer(MoveTarget("other", null, "", fit = false))
        assertEquals(listOf("other"), opened)
        assertEquals(before, viewport.transform)
        assertNull(control.pendingLock)
        assertEquals(0, grids)
    }

    private companion object {
        const val EPS = 1e-2f
    }
}
