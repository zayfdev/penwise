package dev.penwise.sync

private const val ID_BYTES = 16
private const val HEX_RADIX = 16
private const val NIBBLE = 4
private const val LOW_NIBBLE = 0x0F
private const val ULID_LEN = 26
private const val ULID_BITS = 5
private const val ULID_MASK = 31
private const val ULID_TOP_MAX = 7
private const val CROCKFORD = "0123456789ABCDEFGHJKMNPQRSTVWXYZ"

/**
 * Notebook and page ids are 16 raw bytes inside Rust; the UI and the preferences carry them as 32 lowercase hex
 * characters. The server's HTTP routes speak the same 16 bytes as 26-character Crockford base32 ULIDs
 * (`penwise-proto` `ids.rs`): [ulid] and [fromUlid] convert, and these functions are the only conversions.
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

    /** The server's text form of a hex id: 26 uppercase Crockford base32 characters; null for a malformed hex id. */
    fun ulid(hex: String): String? {
        val bytes = bytes(hex) ?: return null
        val value = java.math.BigInteger(1, bytes)
        return buildString(ULID_LEN) {
            for (index in 0 until ULID_LEN) {
                val shift = ULID_BITS * (ULID_LEN - 1 - index)
                append(CROCKFORD[value.shiftRight(shift).toInt() and ULID_MASK])
            }
        }
    }

    /**
     * The hex id of a server ULID (case-insensitive; `I`/`L` read as 1 and `O` as 0, like the server); null when
     * [text] is not 26 Crockford characters whose first is at most `7` (the value must fit 128 bits).
     */
    fun fromUlid(text: String): String? {
        if (text.length != ULID_LEN) return null
        var value = java.math.BigInteger.ZERO
        for ((index, char) in text.withIndex()) {
            val digit = CROCKFORD.indexOf(alias(char.uppercaseChar()))
            if (digit < 0 || (index == 0 && digit > ULID_TOP_MAX)) return null
            value = value.shiftLeft(ULID_BITS).or(java.math.BigInteger.valueOf(digit.toLong()))
        }
        val raw = value.toByteArray().takeLast(ID_BYTES)
        return hex(ByteArray(ID_BYTES - raw.size) + raw.toByteArray())
    }

    private fun alias(char: Char): Char =
        when (char) {
            'O' -> '0'
            'I', 'L' -> '1'
            else -> char
        }
}
