package dev.penwise.sync

import dev.penwise.ink.PageTemplate
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

/** The sync engine against the real `penwise-server`, with the real Rust core on both ends. */
class SyncEngineTest {
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

    private fun device(
        dir: java.io.File =
            java.nio.file.Files
                .createTempDirectory("penwise-tablet")
                .toFile(),
        target: TestServer? = server,
    ) = Device(dir, target).also(open::add)

    @Test
    fun local_edits_persist_and_are_pushed_again_after_a_restart() {
        val first = device(target = null)
        val notebook = first.library.newNotebook("Work")
        val page = requireNotNull(first.library.newPage("Sketch", PageTemplate.Blank, notebook))
        first.scene(page).use { scene ->
            first.draw(scene, 100f, seed = 1)
            first.draw(scene, 200f, seed = 2)
        }
        assertEquals("notebook, page, two strokes", 4, first.pending())
        first.engine.flush()
        first.close()

        // A new process: the state comes back from the file alone.
        val second = device(dir = first.dir)
        assertEquals("the outbox survived", 4, second.pending())
        assertEquals(
            listOf("Work"),
            second.library
                .list(null)
                .books
                .map { it.title },
        )
        assertEquals(2, second.items(page))
        second.connect()
        eventually("outbox acknowledged") { second.pending() == 0 }

        // Another device with no files learns everything from the server.
        val other = device()
        other.connect()
        eventually("catalog replicated") {
            other.library
                .list(null)
                .all()
                .any { it.id == page }
        }
        eventually("strokes replicated") { other.items(page) == 2 }
        assertEquals(0, other.core.outbox().pending)
    }

    @Test
    fun a_remote_catalog_change_arrives_live() {
        val a = device()
        val b = device()
        a.connect()
        b.connect()
        val notebook = a.library.newNotebook("Shared")
        val page = requireNotNull(a.library.newPage("One", PageTemplate.Ruled, notebook))
        eventually("b sees the page") {
            b.library
                .list(null)
                .page(page)
                ?.background == "ruled"
        }

        a.library.renamePage(page, "Renamed")
        a.library.newPage("Two", PageTemplate.Infinite, notebook)
        eventually("b sees the rename and the new page") {
            val list = b.library.list(null)
            list.page(page)?.title == "Renamed" && list.all().size == 2
        }
        assertTrue(b.recorder.outcomes.any { it.catalogChanged })

        assertTrue(a.library.deletePage(page))
        eventually("b sees the delete") { b.library.list(null).page(page) == null }
    }

    @Test
    fun offline_work_on_two_devices_merges_after_reconnect() {
        val a = device()
        val b = device()
        a.connect()
        val notebook = a.library.newNotebook("Base")
        val page = requireNotNull(a.library.newPage("P", PageTemplate.Blank, notebook))
        eventually("a settled") { a.pending() == 0 }
        b.connect()
        eventually("b has the page") { b.library.list(null).page(page) != null }
        b.engine.flush()

        // Both draw on the same page while neither is connected to anything new: stop b's socket first.
        val offline = device(dir = b.dir, target = null)
        b.close()
        a.scene(page).use { a.draw(it, 100f, seed = 11) }
        offline.scene(page).use { offline.draw(it, 300f, seed = 12) }
        offline.engine.flush()
        offline.close()

        val back = device(dir = b.dir)
        back.connect()
        eventually("strokes converge on both") { a.items(page) == 2 && back.items(page) == 2 }
        eventually("nothing left to send") { a.pending() == 0 && back.pending() == 0 }
    }

    @Test
    fun a_wrong_token_is_reported_and_nothing_is_applied() {
        val tablet = Device(server = server) { "not-the-token" }.also(open::add)
        tablet.connect()
        eventually("bad token status") { tablet.engine.status == SyncStatus.BAD_TOKEN }
        assertEquals(0L, tablet.core.headSeq())
    }

    @Test
    fun a_different_server_keeps_unsent_work() {
        val tablet = device()
        tablet.connect()
        val notebook = tablet.library.newNotebook("Old server")
        tablet.library.newPage("Old page", PageTemplate.Blank, notebook)
        eventually("synced") { tablet.pending() == 0 }
        tablet.engine.flush()
        tablet.close()

        val port = server.port
        server.close()
        Thread.sleep(RESTART_PAUSE_MS)
        val fresh = TestServer.start(port = port)
        open.add(fresh)
        val unsent = device(dir = tablet.dir, target = null)
        unsent.library.newNotebook("Unsent")
        unsent.engine.flush()
        unsent.close()

        val back = Device(dir = tablet.dir, server = fresh).also(open::add)
        back.connect()
        eventually("replica reset to the new server, unsent notebook kept and pushed") {
            back.pending() == 0 && back.library
                .list(null)
                .books
                .map { it.title } == listOf("Unsent")
        }
    }

    private companion object {
        const val RESTART_PAUSE_MS = 400L
    }
}
