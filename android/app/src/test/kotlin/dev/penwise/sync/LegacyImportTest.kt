package dev.penwise.sync

import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.ByteArrayOutputStream
import java.io.DataOutputStream
import java.io.File
import java.nio.file.Files

/** The one-time import of the pre-rewrite files, with an old-layout `.bin` built by hand. */
class LegacyImportTest {
    private val open = ArrayList<AutoCloseable>()

    @After
    fun closeAll() = open.reversed().forEach(AutoCloseable::close)

    /** The old `StrokeFile` layout: `PWST`, version 1, then big-endian records. */
    private fun pwst(
        seeds: List<Long>,
        samplesPerStroke: Int = 10,
    ): ByteArray {
        val bytes = ByteArrayOutputStream()
        DataOutputStream(bytes).use { out ->
            out.write("PWST".toByteArray())
            out.writeByte(1)
            for ((row, seed) in seeds.withIndex()) {
                out.writeByte(1)
                out.writeInt(0)
                out.writeInt(0xFF000000.toInt())
                out.writeFloat(1f)
                out.writeLong(seed)
                out.writeInt(samplesPerStroke)
                for (i in 0 until samplesPerStroke) {
                    out.writeFloat(30f + i * 7f)
                    out.writeFloat(40f + row * 50f + (i % 3) * 5f)
                    out.writeFloat(0.5f)
                    out.writeFloat(0f)
                    out.writeFloat(0f)
                    out.writeInt(i * 8_000)
                }
            }
        }
        return bytes.toByteArray()
    }

    private fun oldFiles(
        index: Boolean = true,
        vararg pages: Pair<String, ByteArray>,
    ): File {
        val dir = Files.createTempDirectory("penwise-old").toFile()
        File(dir, "pages").mkdirs()
        pages.forEach { (id, bytes) -> File(dir, "pages/$id.bin").writeBytes(bytes) }
        if (index) {
            File(dir, "meta.json").writeText(
                """{"version":1,"active":"pg_a","notebooks":[
                  {"id":"nb_one","title":"School","pages":[
                    {"id":"pg_a","title":"Maths","kind":{"t":"fixed","width":2560,"height":1600},"background":"grid"},
                    {"id":"pg_b","title":"Scratch","kind":{"t":"infinite"},"background":"blank"}]}]}""",
            )
        }
        return dir
    }

    private fun device(dir: File) = Device(dir, server = null).also(open::add)

    @Test
    fun a_fixture_file_becomes_the_right_number_of_items_under_deterministic_ids() {
        val dir = oldFiles(pages = arrayOf("pg_a" to pwst(listOf(-5L, 77L, Long.MAX_VALUE))))
        val tablet = device(dir)
        val legacy = tablet.engine.legacy
        assertTrue("old files make the import pending", legacy.pending())

        val report = legacy.run(tablet.core)
        assertEquals(LegacyReport(pages = 2, added = 3, alreadyPresent = 0, skipped = 0), report)

        val catalog = tablet.core.catalog()
        val maths = requireNotNull(catalog.page(SyncCore.legacyPageId("pg_a")))
        assertEquals("Maths", maths.title)
        assertEquals(2, maths.background)
        assertFalse(maths.infinite)
        assertEquals(SyncCore.legacyNotebookId("nb_one"), maths.notebook)
        assertEquals(true, catalog.page(SyncCore.legacyPageId("pg_b"))?.infinite)
        assertEquals(3, tablet.items(maths.id))
        assertEquals("empty pages are created without strokes", 0, tablet.items(SyncCore.legacyPageId("pg_b")))
        assertEquals("notebook, two pages, three strokes", 6, tablet.pending())
    }

    @Test
    fun importing_twice_adds_nothing_and_the_marker_ends_the_pending_state() {
        val dir = oldFiles(pages = arrayOf("pg_a" to pwst(listOf(1L, 2L))))
        val tablet = device(dir)
        tablet.engine.legacy.run(tablet.core)
        val again = tablet.engine.legacy.run(tablet.core)
        assertEquals(0, again.added)
        assertEquals(2, again.alreadyPresent)
        tablet.engine.legacy.done()
        assertFalse(tablet.engine.legacy.pending())
        assertTrue("the old files stay as a backup", File(dir, "pages/pg_a.bin").isFile)
        assertTrue(File(dir, "meta.json").isFile)
    }

    @Test
    fun without_an_index_every_page_file_lands_in_one_notebook() {
        val dir = oldFiles(index = false, "pg_x" to pwst(listOf(9L)), "pg_y" to pwst(listOf(8L, 7L)))
        val tablet = device(dir)
        val report = tablet.engine.legacy.run(tablet.core)
        assertEquals(3, report.added)
        assertEquals(
            1,
            tablet.core
                .catalog()
                .liveNotebooks.size,
        )
        assertEquals(listOf(1, 2), listOf("pg_x", "pg_y").map { tablet.items(SyncCore.legacyPageId(it)) })
    }

    @Test
    fun a_fresh_install_has_nothing_to_import() {
        val tablet = device(Files.createTempDirectory("penwise-new").toFile())
        assertFalse(tablet.engine.legacy.pending())
    }

    @Test
    fun the_import_survives_a_restart_and_reaches_the_server() {
        val server = TestServer.start().also(open::add)
        val dir = oldFiles(pages = arrayOf("pg_a" to pwst(listOf(3L, 4L, 5L, 6L))))
        val first = Device(dir, server).also(open::add)
        first.connect()
        eventually("first catch-up") { first.recorder.caughtUp > 0 }
        first.engine.legacy.run(first.core)
        first.engine.flush()
        eventually("imported ops acknowledged") { first.pending() == 0 }

        val other = Device(server = server).also(open::add)
        other.connect()
        val page = SyncCore.legacyPageId("pg_a")
        eventually("a second device sees the imported strokes") { other.items(page) == 4 }

        // A second device with the same old files imports after catch-up and adds nothing (union by op id).
        val twin = Device(oldFiles(pages = arrayOf("pg_a" to pwst(listOf(3L, 4L, 5L, 6L)))), server).also(open::add)
        twin.connect()
        eventually("twin caught up") { twin.recorder.caughtUp > 0 && twin.items(page) == 4 }
        assertEquals(
            0,
            twin.engine.legacy
                .run(twin.core)
                .added,
        )
        assertEquals(0, twin.pending())
    }
}
