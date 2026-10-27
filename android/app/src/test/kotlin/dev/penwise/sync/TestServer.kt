package dev.penwise.sync

import java.io.File
import java.net.InetSocketAddress
import java.net.ServerSocket
import java.net.Socket
import java.nio.file.Files

/** The real `penwise-server` binary on a free loopback port with a throwaway data directory. */
class TestServer private constructor(
    val port: Int,
    val dataDir: File,
    private val process: Process,
) : AutoCloseable {
    val url: String get() = "ws://127.0.0.1:$port/sync"

    /** The shared token the server generated into `<data>/token`. */
    val token: String get() = File(dataDir, "token").readText().trim()

    override fun close() {
        process.destroy()
        process.waitFor()
    }

    companion object {
        private const val START_TIMEOUT_MS = 20_000L
        private const val POLL_MS = 50L

        /** Starts a server; `port`/`dataDir` pin them (restart on the same port with new data). */
        fun start(
            port: Int = freePort(),
            dataDir: File = Files.createTempDirectory("penwise-server").toFile(),
        ): TestServer {
            val binary = requireNotNull(System.getProperty("penwise.server.bin")) { "penwise.server.bin is not set" }
            val builder =
                ProcessBuilder(binary)
                    .redirectErrorStream(true)
                    .redirectOutput(File(dataDir, "server.log"))
            builder.environment().apply {
                put("PENWISE_HOST", "127.0.0.1")
                put("PENWISE_PORT", port.toString())
                put("PENWISE_DATA", dataDir.absolutePath)
                put("PENWISE_WEB", File(dataDir, "no-web").absolutePath)
            }
            val process = builder.start()
            val server = TestServer(port, dataDir, process)
            val deadline = System.currentTimeMillis() + START_TIMEOUT_MS
            while (!accepting(port)) {
                check(process.isAlive) { "penwise-server exited: ${File(dataDir, "server.log").readText()}" }
                check(System.currentTimeMillis() < deadline) { "penwise-server did not start" }
                Thread.sleep(POLL_MS)
            }
            return server
        }

        private fun freePort(): Int = ServerSocket(0).use { it.localPort }

        private fun accepting(port: Int): Boolean =
            try {
                Socket().use { it.connect(InetSocketAddress("127.0.0.1", port), POLL_MS.toInt()) }
                true
            } catch (_: java.io.IOException) {
                false
            }
    }
}
