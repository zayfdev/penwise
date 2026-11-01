package dev.penwise.frames

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class RequestShapeTest {
    @Test
    fun bodilessPostSendsJsonEmptyObject() {
        val shape = RequestShape.of("POST", null)!!
        assertEquals("application/json", shape.contentType)
        assertEquals("{}", String(shape.body, Charsets.UTF_8))
    }

    @Test
    fun bodilessPutSendsJsonEmptyObject() {
        val shape = RequestShape.of("PUT", null)!!
        assertEquals("application/json", shape.contentType)
        assertEquals("{}", String(shape.body, Charsets.UTF_8))
    }

    @Test
    fun givenBodyIsSentAsJsonWithItsOwnLength() {
        val body = "{\"route\":\"/é\"}".toByteArray(Charsets.UTF_8)
        val shape = RequestShape.of("PUT", body)!!
        assertEquals("application/json", shape.contentType)
        assertEquals(body.size, shape.body.size)
    }

    @Test
    fun getCarriesNothing() {
        assertNull(RequestShape.of("GET", null))
    }
}
