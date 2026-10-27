package dev.penwise.ink

/** State of one shape drag: where it started and the throwaway preview built for the current drag box. */
internal class ShapeDrag<T : AutoCloseable> {
    var start: FloatArray? = null
        private set
    private var temp: T? = null

    val active: Boolean get() = start != null

    fun begin(
        x: Float,
        y: Float,
    ) {
        abandon()
        start = floatArrayOf(x, y)
    }

    /** Swaps in the newest preview, closing the one it replaces. */
    fun replaceTemp(next: T) {
        temp?.close()
        temp = next
    }

    /** Ends the drag normally and hands the preview (if any) to the caller, who must close it. */
    fun finish(): T? {
        val last = temp
        temp = null
        start = null
        return last
    }

    /** Drops the drag without producing anything: closes the preview and forgets the start. */
    fun abandon() {
        temp?.close()
        temp = null
        start = null
    }
}
