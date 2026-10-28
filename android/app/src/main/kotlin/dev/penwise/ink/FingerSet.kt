package dev.penwise.ink

import kotlin.math.abs
import kotlin.math.hypot

/** The fingers currently down (up to [MAX]) with their latest and starting positions. Pure bookkeeping. */
internal class FingerSet {
    private val ids = IntArray(MAX)
    private val xs = FloatArray(MAX)
    private val ys = FloatArray(MAX)
    private val startX = FloatArray(MAX)
    private val startY = FloatArray(MAX)

    var count = 0
        private set

    val isFull: Boolean get() = count == MAX

    val midX: Float get() = (xs[0] + xs[1]) / 2f
    val midY: Float get() = (ys[0] + ys[1]) / 2f

    /** Distance between the first two fingers. */
    val span: Float get() = hypot(xs[0] - xs[1], ys[0] - ys[1])

    fun id(index: Int): Int = ids[index]

    fun indexOf(id: Int): Int {
        for (i in 0 until count) if (ids[i] == id) return i
        return -1
    }

    fun add(
        id: Int,
        x: Float,
        y: Float,
    ) {
        ids[count] = id
        xs[count] = x
        ys[count] = y
        startX[count] = x
        startY[count] = y
        count++
    }

    fun moveTo(
        index: Int,
        x: Float,
        y: Float,
    ) {
        xs[index] = x
        ys[index] = y
    }

    fun remove(index: Int) {
        for (i in index until count - 1) {
            ids[i] = ids[i + 1]
            xs[i] = xs[i + 1]
            ys[i] = ys[i + 1]
            startX[i] = startX[i + 1]
            startY[i] = startY[i + 1]
        }
        count--
    }

    fun clear() {
        count = 0
    }

    /** Makes the current positions the ones travel is measured from. */
    fun restart() {
        for (k in 0 until count) {
            startX[k] = xs[k]
            startY[k] = ys[k]
        }
    }

    /** Has any finger travelled farther than [slop] from its starting position? */
    fun movedBeyond(slop: Float): Boolean {
        for (i in 0 until count) {
            if (abs(xs[i] - startX[i]) > slop || abs(ys[i] - startY[i]) > slop) return true
        }
        return false
    }

    companion object {
        const val MAX = 10
    }
}
