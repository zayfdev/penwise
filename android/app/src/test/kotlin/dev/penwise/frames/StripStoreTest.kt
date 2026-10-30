package dev.penwise.frames

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

/** The strip files: content-addressed reuse, atomic writes, and a keep-set judged when files are deleted. */
class StripStoreTest {
    @get:Rule
    val folder = TemporaryFolder()

    private var now = 1_000_000L
    private val store by lazy { StripStore(folder.newFolder("strips"), { now }, graceMs = 5_000L) }
    private val a = "/frames/f/strip/aaaa.png"
    private val b = "/frames/f/strip/bbbb.png"
    private val c = "/frames/f/strip/cccc.png"

    private fun names() =
        store
            .file(a)
            .parentFile
            ?.list()
            .orEmpty()
            .sorted()

    @Test
    fun a_new_still_keeps_the_strips_it_shares_with_the_old_one() {
        assertTrue(store.retain("f", listOf(a, b)).isEmpty())
        // The page changed below the fold: strip b is new content, a is the same bytes under the same address.
        val gone = store.retain("f", listOf(a, c))
        assertEquals(setOf(b), gone)
    }

    @Test
    fun a_strip_another_frame_still_shows_is_not_gone() {
        store.retain("f", listOf(a, b))
        store.retain("g", listOf(b))
        assertEquals(setOf(a), store.retain("f", emptyList()))
        assertEquals(setOf(b), store.retain("g", emptyList()))
    }

    @Test
    fun a_write_leaves_the_whole_file_or_none_and_no_temporary_file() {
        assertTrue(store.write(a, byteArrayOf(1, 2, 3)))
        assertTrue(store.has(a))
        assertEquals(listOf(3), listOf(store.file(a).length().toInt()))
        assertEquals(1, names().size)
        assertFalse(names().any { it.contains(".tmp") })
    }

    @Test
    fun purge_deletes_what_no_still_lists_but_only_once_it_is_older_than_the_grace() {
        store.retain("f", listOf(a))
        store.write(a, byteArrayOf(1))
        store.write(b, byteArrayOf(2))
        // b was written a moment ago: it may be about to be listed by a still that has not arrived yet.
        store.purge()
        assertTrue(store.has(b))
        now += 6_000
        store.file(b).setLastModified(now - 6_000)
        store.file(a).setLastModified(now - 6_000)
        store.purge()
        assertFalse("no still lists b", store.has(b))
        assertTrue("a is in use", store.has(a))
    }

    @Test
    fun the_keep_set_is_judged_when_files_are_deleted_not_when_the_purge_was_asked_for() {
        store.write(b, byteArrayOf(2))
        store.file(b).setLastModified(now - 60_000)
        store.purge()
        // A still that lists b arrives; a purge asked for earlier would have used the old list.
        store.write(c, byteArrayOf(3))
        store.file(c).setLastModified(now - 60_000)
        store.retain("f", listOf(c))
        store.purge()
        assertTrue(store.has(c))
        assertFalse(store.has(b))
    }

    @Test
    fun a_temporary_file_left_by_a_dead_write_is_cleaned_up_after_a_minute() {
        store.write(a, byteArrayOf(1))
        val stale = java.io.File(store.file(a).parentFile, "x.png.tmp9")
        stale.writeBytes(byteArrayOf(0))
        stale.setLastModified(now - 120_000)
        store.retain("f", listOf(a))
        store.file(a).setLastModified(now - 60_000)
        store.purge()
        assertFalse(stale.exists())
        assertTrue(store.has(a))
    }
}
