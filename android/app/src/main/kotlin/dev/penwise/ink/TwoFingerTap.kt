package dev.penwise.ink

import android.view.MotionEvent
import kotlin.math.abs

/**
 * Detects a quick two-finger tap (both fingers down and up within [TIMEOUT_MS], neither moving more than [slopPx]).
 * Any stylus pointer or a third contact invalidates the gesture.
 */
internal class TwoFingerTap(
    private val slopPx: Float,
    private val onTap: () -> Unit,
) {
    private val startX = FloatArray(MAX_ID)
    private val startY = FloatArray(MAX_ID)
    private var downTime = 0L
    private var fingers = 0
    private var valid = false

    fun onTouchEvent(event: MotionEvent) {
        when (event.actionMasked) {
            MotionEvent.ACTION_DOWN -> {
                valid = true
                fingers = 0
                downTime = event.eventTime
                addPointer(event)
            }

            MotionEvent.ACTION_POINTER_DOWN -> {
                addPointer(event)
            }

            MotionEvent.ACTION_MOVE -> {
                if (valid && moved(event)) valid = false
            }

            MotionEvent.ACTION_UP -> {
                if (valid && fingers == 2 && event.eventTime - downTime <= TIMEOUT_MS) onTap()
                valid = false
            }

            MotionEvent.ACTION_CANCEL -> {
                valid = false
            }
        }
    }

    private fun addPointer(event: MotionEvent) {
        val index = event.actionIndex
        val id = event.getPointerId(index)
        if (event.getToolType(index) != MotionEvent.TOOL_TYPE_FINGER || id >= MAX_ID || fingers >= 2) {
            valid = false
        } else {
            startX[id] = event.getX(index)
            startY[id] = event.getY(index)
            fingers++
        }
    }

    private fun moved(event: MotionEvent): Boolean {
        for (i in 0 until event.pointerCount) {
            val id = event.getPointerId(i)
            if (id >= MAX_ID ||
                abs(event.getX(i) - startX[id]) > slopPx ||
                abs(event.getY(i) - startY[id]) > slopPx
            ) {
                return true
            }
        }
        return false
    }

    private companion object {
        const val MAX_ID = 16
        const val TIMEOUT_MS = 300L
    }
}
