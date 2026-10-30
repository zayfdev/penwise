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

/** Camera moves other devices ask for: `Focus` fits a frame, an unknown item fits the content, a drawing hand waits. */
class FrameAgentTest {
    private val job = Job()
    private val pen = PenActivity()
    private var now = 100_000L
    private var current = "page"
    private val opened = ArrayList<String>()
    private var frames: List<FrameSpec> = emptyList()
    private var bounds: PageRect? = null
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
            control = FrameControl(),
            page =
                AgentPage(
                    current = { current },
                    open = { opened += it },
                    title = { "Title $it" },
                    specs = { frames },
                    contentBounds = { bounds },
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
    fun a_focus_fits_the_frame_width_to_the_screen() {
        frames = listOf(frame("a", 0f, 0f), frame("b", 2000f, 100f))
        agent.offer(MoveTarget("page", "b", ""))
        assertEquals(2560f / 1440f, viewport.transform.scale, EPS)
        assertEquals(2000f, viewport.transform.toPageX(0f), EPS)
        assertEquals(100f, viewport.transform.toPageY(0f), EPS)
        assertTrue(opened.isEmpty())
    }

    @Test
    fun an_item_that_is_no_frame_fits_everything_on_the_page_not_only_its_frames() {
        frames = listOf(frame("a", 0f, 0f), frame("b", 2000f, 0f))
        // The core's bounds of all layers: the frames, and ink and a picture well beyond them.
        bounds = PageRect(-600f, -400f, 9_000f, 7_000f)
        agent.offer(MoveTarget("page", "not-a-frame", ""))
        val view = viewport.transform
        assertTrue(view.toScreenX(-600f) >= -EPS && view.toScreenX(8_400f) <= 2560f + EPS)
        assertTrue(view.toScreenY(-400f) >= -EPS && view.toScreenY(6_600f) <= 1600f + EPS)
        assertEquals("the height decides: 1600 / 7000", 1600f / 7_000f, view.scale, EPS)
    }

    @Test
    fun a_page_with_nothing_on_it_resets_the_camera() {
        viewport.transformBy(2f, 0f, 0f, 100f, 100f)
        bounds = null
        agent.offer(MoveTarget("page", null, ""))
        assertEquals(1f, viewport.transform.scale, EPS)
        assertEquals(0f, viewport.transform.tx, EPS)
    }

    @Test
    fun a_plain_page_switch_changes_the_page_and_leaves_the_camera_alone() {
        frames = listOf(frame("a", 0f, 0f))
        val before = viewport.transform
        agent.offer(MoveTarget("other", null, "", fit = false))
        assertEquals(listOf("other"), opened)
        assertEquals(before, viewport.transform)
    }

    @Test
    fun while_the_owner_draws_nothing_moves_and_the_chip_offers_the_move() {
        frames = listOf(frame("a", 0f, 0f), frame("b", 2000f, 0f))
        pen.lastDrawAt = now - 4_000
        val before = viewport.transform
        agent.offer(MoveTarget("page", "b", ""))
        assertEquals(before, viewport.transform)
        val chip = agent.chip.value
        assertNotNull(chip)
        assertEquals("/b", chip?.label)
        agent.accept()
        assertNull(agent.chip.value)
        assertEquals(2000f, viewport.transform.toPageX(0f), EPS)
    }

    @Test
    fun a_move_ten_quiet_seconds_after_the_last_stroke_just_happens() {
        frames = listOf(frame("a", 0f, 0f), frame("b", 2000f, 0f))
        pen.lastDrawAt = now - 10_000
        agent.offer(MoveTarget("page", "b", ""))
        assertNull(agent.chip.value)
        assertEquals(2000f, viewport.transform.toPageX(0f), EPS)
    }

    @Test
    fun a_move_to_another_page_fits_once_that_pages_frames_have_arrived() {
        frames = emptyList()
        agent.offer(MoveTarget("other", "b", ""))
        assertEquals(listOf("other"), opened)
        assertEquals(1f, viewport.transform.scale, EPS)
        // The new page opens and its first sweep finds the frame.
        current = "other"
        frames = listOf(frame("b", 2000f, 0f))
        agent.framesChanged()
        assertEquals(2000f, viewport.transform.toPageX(0f), EPS)
    }

    @Test
    fun the_chip_can_be_dismissed() {
        pen.lastDrawAt = now - 1_000
        agent.offer(MoveTarget("page", null, ""))
        agent.dismiss()
        assertNull(agent.chip.value)
        agent.accept()
        assertTrue(opened.isEmpty())
    }

    private companion object {
        const val EPS = 1e-2f
    }
}
