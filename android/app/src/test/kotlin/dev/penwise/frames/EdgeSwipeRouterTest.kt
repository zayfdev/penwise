package dev.penwise.frames

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** The router: the page keeps the touch until a horizontal edge swipe is recognised, then it is cancelled and ours. */
class EdgeSwipeRouterTest {
    private var enabled = true
    private val log = ArrayList<String>()
    private val swipes = ArrayList<Int>()
    private val router =
        EdgeSwipeRouter(
            EdgeSwipe(edgePx = 48f, travelPx = 112f, stripPx = 400f),
            { enabled },
            { 2000f to 1200f },
            { log += "cancel" },
        ) { swipes += it }

    private fun send(
        kind: TouchKind,
        x: Float,
        y: Float = 600f,
        pointers: Int = 1,
        finger: Boolean = true,
    ): Boolean =
        router.route(EdgeTouch(kind, x, y, pointers, finger)) {
            log += kind.name
            true
        }

    @Test
    fun the_page_sees_an_edge_touch_until_it_turns_into_a_swipe() {
        assertTrue(send(TouchKind.DOWN, 10f))
        assertTrue(send(TouchKind.MOVE, 60f))
        assertEquals(listOf("DOWN", "MOVE"), log)
        assertTrue(swipes.isEmpty())
    }

    @Test
    fun a_recognised_swipe_cancels_the_page_and_claims_the_rest_of_the_touch() {
        send(TouchKind.DOWN, 10f)
        assertTrue(send(TouchKind.MOVE, 140f))
        assertEquals(listOf(EdgeSwipe.PREVIOUS), swipes)
        assertEquals(listOf("DOWN", "cancel"), log)
        assertTrue(router.claiming)
        assertTrue(send(TouchKind.MOVE, 300f))
        assertTrue(send(TouchKind.UP, 300f))
        assertFalse(router.claiming)
        assertEquals("the page saw nothing after the cancel", listOf("DOWN", "cancel"), log)
    }

    @Test
    fun a_touch_outside_the_strips_or_with_nothing_to_switch_to_is_the_pages() {
        send(TouchKind.DOWN, 500f)
        send(TouchKind.MOVE, 900f)
        enabled = false
        send(TouchKind.UP, 900f)
        send(TouchKind.DOWN, 10f)
        send(TouchKind.MOVE, 300f)
        assertTrue(swipes.isEmpty())
        assertFalse(router.claiming)
    }

    @Test
    fun a_second_finger_ends_the_classification_and_the_page_hands_over() {
        send(TouchKind.DOWN, 10f)
        send(TouchKind.OTHER, 10f, pointers = 2)
        send(TouchKind.MOVE, 300f, pointers = 2)
        assertTrue(swipes.isEmpty())
        assertEquals(listOf("DOWN", "OTHER", "MOVE"), log)
    }

    @Test
    fun a_palm_or_stylus_never_starts_a_swipe() {
        send(TouchKind.DOWN, 10f, finger = false)
        send(TouchKind.MOVE, 300f)
        assertTrue(swipes.isEmpty())
    }

    @Test
    fun a_vertical_drag_from_the_edge_stays_the_pages() {
        send(TouchKind.DOWN, 10f)
        send(TouchKind.MOVE, 30f, y = 1000f)
        assertTrue(swipes.isEmpty())
        assertEquals(listOf("DOWN", "MOVE"), log)
    }
}
