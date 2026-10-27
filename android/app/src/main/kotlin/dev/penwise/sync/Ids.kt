package dev.penwise.sync

private const val ID_BYTES = 16
private const val HEX_RADIX = 16
private const val NIBBLE = 4
private const val LOW_NIBBLE = 0x0F

/**
 * Notebook and page ids are 16 raw bytes inside Rust; the UI and the preferences carry them as 32 lowercase hex
 * characters. These two functions are the only conversion.
 */
object Ids {
    /** `bytes` as lowercase hex. */
    fun hex(bytes: ByteArray): String =
        buildString(bytes.size * 2) {
            bytes.forEach { byte ->
                val value = byte.toInt() and 0xFF
                append(Character.forDigit(value ushr NIBBLE, HEX_RADIX))
                append(Character.forDigit(value and LOW_NIBBLE, HEX_RADIX))
            }
        }

    /** The 16 bytes of a hex id; `null` when `text` is not exactly 32 hex characters. */
    fun bytes(text: String): ByteArray? {
        if (text.length != ID_BYTES * 2) return null
        val out = ByteArray(ID_BYTES)
        for (index in out.indices) {
            val high = Character.digit(text[index * 2], HEX_RADIX)
            val low = Character.digit(text[index * 2 + 1], HEX_RADIX)
            if (high < 0 || low < 0) return null
            out[index] = ((high shl NIBBLE) or low).toByte()
        }
        return out
    }
}
