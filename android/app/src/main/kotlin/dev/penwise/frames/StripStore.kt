package dev.penwise.frames

import java.io.File

/**
 * The strip PNGs on disk and which frames use which. A strip's URL is content-addressed
 * (`/frames/{id}/strip/{hash}.png`, immutable), so the URL names the file and a new still reuses every strip whose
 * content did not change. Files are written to a temporary name and renamed into place, so a reader never sees half a
 * PNG; what is kept is decided when files are deleted, under the same lock that registers a still's strips, and a
 * file younger than [graceMs] is never deleted (it may be on its way to a strip that is about to be registered).
 * Thread-safe.
 */
internal class StripStore(
    private val dir: File,
    private val now: () -> Long = System::currentTimeMillis,
    private val graceMs: Long = GRACE_MS,
) {
    private val lock = Any()
    private val owned = HashMap<String, Set<String>>()
    private var counter = 0

    /** The file a strip is kept in. */
    fun file(url: String): File = File(dir, fileName(url))

    /** True when the strip is on disk. */
    fun has(url: String): Boolean = file(url).isFile

    /**
     * Writes the strip's bytes, atomically: to a temporary file in the same directory, then renamed over the real
     * name. False when it could not be written.
     */
    fun write(
        url: String,
        bytes: ByteArray,
    ): Boolean {
        dir.mkdirs()
        val target = file(url)
        val temp = File(dir, "${target.name}$TEMP${nextCounter()}")
        return try {
            temp.writeBytes(bytes)
            temp.renameTo(target).also { if (!it) temp.delete() }
        } catch (_: java.io.IOException) {
            temp.delete()
            false
        }
    }

    /**
     * [frame]'s current still uses exactly [urls]. Returns the strips nobody uses any more (not this frame, not
     * another), whose bitmaps can go; strips the new still shares with the old one are not among them.
     */
    fun retain(
        frame: String,
        urls: Collection<String>,
    ): Set<String> =
        synchronized(lock) {
            val before = owned.put(frame, urls.toSet()).orEmpty()
            val inUse = owned.values.flatMapTo(HashSet()) { it }
            if (urls.isEmpty()) owned.remove(frame)
            before.filterTo(HashSet()) { it !in inUse }
        }

    /** Deletes files no still uses (this session's or an earlier one's), judged now, and stale temporary files. */
    fun purge() {
        synchronized(lock) {
            val keep = owned.values.flatMapTo(HashSet()) { urls -> urls.map(::fileName) }
            val limit = now() - graceMs
            for (file in dir.listFiles().orEmpty()) {
                val old = file.lastModified() <= limit
                val unused = file.name !in keep && !file.name.contains(TEMP)
                val stale = file.name.contains(TEMP) && file.lastModified() <= now() - TEMP_MS
                if ((old && unused) || stale) file.delete()
            }
        }
    }

    private fun nextCounter(): Int = synchronized(lock) { counter++ }

    private fun fileName(url: String): String =
        url.map { if (it.isLetterOrDigit() || it == '.' || it == '-') it else '_' }.joinToString("")

    private companion object {
        /** A file this young stays even if no still lists it yet. */
        const val GRACE_MS = 5_000L

        /** A temporary file this old is a write that died. */
        const val TEMP_MS = 60_000L
        const val TEMP = ".tmp"
    }
}
