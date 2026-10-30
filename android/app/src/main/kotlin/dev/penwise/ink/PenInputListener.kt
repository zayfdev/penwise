package dev.penwise.ink

/**
 * Told about every stylus touch or hover event with its uptime-millis time. A named interface rather than
 * `(Long) -> Unit`, which would box the timestamp on every pen sample.
 */
fun interface PenInputListener {
    fun onPenInput(eventTime: Long)
}
