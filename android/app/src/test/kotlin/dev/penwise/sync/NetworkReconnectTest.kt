package dev.penwise.sync

import org.junit.Assert.assertEquals
import org.junit.Test
import java.nio.file.Files

/** A usable network redials at once, however deep the socket's backoff has grown. */
class NetworkReconnectTest {
    private class FakeNetwork : NetworkSource {
        var fire: (Any) -> Unit = {}
        var closed = false
        var active: Any? = null

        override fun active(): Any? = active

        override fun watch(onValidated: (network: Any) -> Unit): AutoCloseable {
            fire = onValidated
            return AutoCloseable { closed = true }
        }
    }

    @Test
    fun a_validated_network_skips_the_backoff_wait() {
        val dataDir = Files.createTempDirectory("penwise-server").toFile()
        val first = TestServer.start(dataDir = dataDir)
        val port = first.port
        first.close()
        val network = FakeNetwork()
        val tablet = Device(server = first, network = network)
        try {
            tablet.connect()
            // Failed dials at 0, 1 and 3 s: the next one would be at 7 s, 3.5 s after the network returns.
            Thread.sleep(DEEP_BACKOFF_MS)
            val back = TestServer.start(port = port, dataDir = dataDir)
            try {
                val started = System.currentTimeMillis()
                network.fire("wifi")
                eventually("online within 2.5 s of the network", timeoutMs = WITHIN_MS) {
                    tablet.engine.status == SyncStatus.ONLINE
                }
                assertEquals(true, System.currentTimeMillis() - started < WITHIN_MS)
            } finally {
                back.close()
            }
        } finally {
            tablet.close()
        }
        assertEquals(true, network.closed)
    }

    @Test
    fun a_dial_that_landed_on_another_network_is_redialled_once_when_the_first_returns() {
        val server = TestServer.start()
        val network = FakeNetwork().apply { active = "cell" }
        val recorder = Device(server = server, network = network)
        try {
            recorder.connect()
            eventually("online on cell") { recorder.engine.status == SyncStatus.ONLINE }
            network.active = "wifi"
            network.fire("wifi")
            eventually("redialled for wifi") { recorder.recorder.statuses.count { it == SyncStatus.ONLINE } == 2 }
            // wifi flaps: the socket now opened on wifi, so nothing more happens
            network.fire("wifi")
            Thread.sleep(SETTLE_MS)
            assertEquals(2, recorder.recorder.statuses.count { it == SyncStatus.ONLINE })
        } finally {
            recorder.close()
            server.close()
        }
    }

    private companion object {
        const val SETTLE_MS = 1_200L
        const val DEEP_BACKOFF_MS = 3_500L
        const val WITHIN_MS = 2_500L
    }
}
