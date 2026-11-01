package dev.penwise.ink

import org.junit.Assert.assertEquals
import org.junit.Test

/** A navigation that is cancelled says so before it ends; one that ends by lifting a finger does not. */
class FingerAbortTest {
    private val log = ArrayList<String>()
    private val nav =
        FingerNavigation(
            SLOP,
            { log += "tap" },
            { log += "step" },
            { log += if (it) "begin" else "end" },
            { log += "abort" },
        )

    private fun startPinch() {
        nav.down(0, 400f, 500f, 0)
        nav.down(1, 600f, 500f, 20)
        nav.move(0, 300f, 500f)
        nav.move(1, 700f, 500f)
        nav.update()
    }

    @Test
    fun lifting_a_finger_ends_the_navigation_without_an_abort() {
        startPinch()
        nav.up(0, 100)
        assertEquals(listOf("begin", "step", "end"), log)
    }

    @Test
    fun a_cancel_aborts_before_it_ends() {
        startPinch()
        nav.cancel()
        assertEquals(listOf("begin", "step", "abort", "end"), log)
    }

    @Test
    fun a_third_finger_aborts() {
        startPinch()
        nav.down(2, 900f, 900f, 120)
        assertEquals(listOf("begin", "step", "abort", "end"), log)
    }

    @Test
    fun a_cancel_with_no_navigation_running_aborts_nothing() {
        nav.down(0, 400f, 500f, 0)
        nav.cancel()
        assertEquals(emptyList<String>(), log)
    }

    private companion object {
        const val SLOP = 10f
    }
}
