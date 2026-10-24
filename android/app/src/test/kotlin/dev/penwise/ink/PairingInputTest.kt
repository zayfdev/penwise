package dev.penwise.ink

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** The sync dialog treats six characters as a pairing code and anything longer as a token. */
class PairingInputTest {
    @Test
    fun six_characters_are_a_code() {
        assertTrue(LibraryClient.looksLikeCode("6WSGM4"))
        assertTrue("lower case is fine", LibraryClient.looksLikeCode("6wsgm4"))
        assertTrue("a typed dash is tolerated", LibraryClient.looksLikeCode("6WS-GM4"))
    }

    @Test
    fun tokens_and_short_input_are_not_codes() {
        assertFalse("43-character tokens stay tokens", LibraryClient.looksLikeCode("a".repeat(43)))
        assertFalse(LibraryClient.looksLikeCode("ABC"))
        assertFalse(LibraryClient.looksLikeCode(""))
    }
}
