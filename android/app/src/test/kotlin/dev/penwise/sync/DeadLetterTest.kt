package dev.penwise.sync

import org.junit.After
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import java.io.ByteArrayOutputStream
import java.io.DataInputStream
import java.util.concurrent.atomic.AtomicLong

/** Ops the server refuses for good stay visible and exportable on the tablet. */
class DeadLetterTest {
    private lateinit var server: TestServer
    private val open = ArrayList<AutoCloseable>()

    @Before
    fun startServer() {
        server = TestServer.start()
    }

    @After
    fun stopAll() {
        open.reversed().forEach(AutoCloseable::close)
        server.close()
    }

    /** A tablet whose clock jumps an hour ahead after the handshake: every op it stamps then is refused for skew. */
    private fun skewedDeviceWithOneDeadLetter(): Device {
        val skew = AtomicLong(0)
        val tablet = Device(server = server, clock = { System.currentTimeMillis() + skew.get() }).also(open::add)
        tablet.connect()
        eventually("caught up") { tablet.core.caughtUp() }
        skew.set(ONE_HOUR_MS)
        tablet.library.newNotebook("Refused")
        eventually("the server's refusal arrives and the op is dead-lettered") { tablet.core.outbox().deadLetters == 1 }
        return tablet
    }

    @Test
    fun a_refused_op_is_listed_with_what_why_and_how_big() {
        val tablet = skewedDeviceWithOneDeadLetter()
        val letter = tablet.core.deadLetters().single()
        assertEquals("a notebook op", 4, letter.kind)
        assertEquals("refused for clock skew", 1, letter.reason)
        assertEquals(16, letter.opId.size)
        assertTrue(letter.sizeBytes > 0)
        assertEquals(letter.sizeBytes, tablet.core.deadLetterBytes(letter.opId).size)
        assertEquals(1, tablet.core.outbox().deadLetters)
    }

    @Test
    fun the_refused_ops_export_to_a_file_and_survive_a_restart() {
        val tablet = skewedDeviceWithOneDeadLetter()
        tablet.engine.flush()
        val letter = tablet.core.deadLetters().single()

        val out = ByteArrayOutputStream()
        assertEquals(1, Exports.deadLetters(tablet.core, out))
        val stream = DataInputStream(out.toByteArray().inputStream())
        assertEquals("PWDL", ByteArray(4).also(stream::readFully).decodeToString())
        assertEquals(1, stream.readUnsignedByte())
        assertEquals(1, stream.readInt())
        assertArrayEquals(letter.opId, ByteArray(16).also(stream::readFully))
        val bytes = ByteArray(stream.readInt()).also(stream::readFully)
        assertArrayEquals(tablet.core.deadLetterBytes(letter.opId), bytes)

        tablet.close()
        val again = Device(tablet.dir, server = null).also(open::add)
        assertEquals("the list is part of the persisted state", 1, again.core.deadLetters().size)

        again.core.removeDeadLetter(letter.opId)
        assertEquals(0, again.core.deadLetters().size)
        assertEquals(0, again.core.outbox().deadLetters)
    }

    private companion object {
        const val ONE_HOUR_MS = 3_600_000L
    }
}
