package dev.penwise.sync

import dev.penwise.ink.PageScene
import java.io.File
import java.nio.file.Files

/** One tablet in a test: its own files directory, a [SyncEngine] on it, and what its listener saw. */
class Device(
    val dir: File = Files.createTempDirectory("penwise-tablet").toFile(),
    private val server: TestServer?,
    clock: () -> Long = System::currentTimeMillis,
    network: NetworkSource? = null,
    private val token: () -> String = { server?.token.orEmpty() },
) : AutoCloseable {
    val recorder = Recorder()
    val engine =
        SyncEngine(
            filesDir = dir,
            endpoints = { listOfNotNull(server?.url) },
            token = token,
            listener = recorder,
            clock = clock,
            network = network,
        )
    val core: SyncCore get() = engine.core
    val library: Library get() = engine.library

    fun connect() = engine.start()

    fun pending(): Int = core.outbox().pending

    /** Opens a scene on `page` (hex id). */
    fun scene(page: String): PageScene = PageScene(core.handle, core.lock, requireNotNull(Ids.bytes(page)), 1L, core)

    /** Draws one wavy stroke on `scene` and queues it for sync. */
    fun draw(
        scene: PageScene,
        y: Float,
        seed: Long,
    ) {
        val count = 12
        val samples = FloatArray(count * 5)
        for (i in 0 until count) {
            samples[i * 5] = 20f + i * 6f
            samples[i * 5 + 1] = y + (i % 3) * 4f
            samples[i * 5 + 2] = 0.5f
        }
        val times = IntArray(count) { it * 8_333 }
        val edits = scene.addStroke(scene.newItemId(), 0, 0xFF000000.toInt(), 1f, seed, samples, times)
        check(scene.commit(edits)) { "the stroke produced no edits" }
    }

    /** Live items of `page`; 0 while the page has not arrived. */
    fun items(page: String): Int =
        if (core.catalog().page(page) == null) 0 else scene(page).use { it.renderOrder().size / ID_BYTES }

    override fun close() = engine.close()

    private companion object {
        const val ID_BYTES = 16
    }
}
