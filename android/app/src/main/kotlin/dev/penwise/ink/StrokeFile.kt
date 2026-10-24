package dev.penwise.ink

import dev.penwise.core.InkGeometry
import dev.penwise.core.InkStroke
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.io.BufferedInputStream
import java.io.BufferedOutputStream
import java.io.DataInputStream
import java.io.DataOutputStream
import java.io.EOFException
import java.io.File
import java.io.FileOutputStream

/**
 * One finished stroke as stored on disk: everything [InkStroke] needs to rebuild its geometry byte for byte
 * (preset, colour, size, seed) plus the raw samples.
 */
class SavedStroke(
    val presetIndex: Int,
    val argb: Int,
    val sizeScale: Float,
    val seed: Long,
    val samples: FloatArray,
    val times: IntArray,
    val count: Int,
) {
    /** Geometry rebuilt from the samples; `null` until [rebuild] or [attachGeometry] ran. */
    @Volatile
    var geometry: InkGeometry? = null
        private set

    /** Records geometry that was just built for this stroke (freshly drawn, not reloaded). */
    fun attachGeometry(built: InkGeometry) {
        geometry = built
    }

    /** Rebuilds geometry through the ink engine; call on a background thread. */
    fun rebuild(): InkGeometry? {
        val built =
            InkStroke(presetIndex, seed, argb, sizeScale).use { stroke ->
                stroke.push(samples, times, count)
                stroke.finish()
            }
        geometry = built
        return built
    }
}

/** Binary codec for one stroke record; big-endian, matching [DataOutputStream]. */
internal object SavedStrokeCodec {
    private const val TAG_STROKE = 1
    private const val MAX_SAMPLES = 1 shl 20
    const val STRIDE = 5

    fun write(
        out: DataOutputStream,
        stroke: SavedStroke,
    ) {
        out.writeByte(TAG_STROKE)
        out.writeInt(stroke.presetIndex)
        out.writeInt(stroke.argb)
        out.writeFloat(stroke.sizeScale)
        out.writeLong(stroke.seed)
        out.writeInt(stroke.count)
        for (i in 0 until stroke.count) {
            val o = i * STRIDE
            for (k in 0 until STRIDE) out.writeFloat(stroke.samples[o + k])
            out.writeInt(stroke.times[i])
        }
    }

    /** Next record, or `null` at a clean end of stream. Throws [IllegalStateException] on a malformed record. */
    fun read(input: DataInputStream): SavedStroke? {
        val tag =
            try {
                input.readUnsignedByte()
            } catch (_: EOFException) {
                return null
            }
        check(tag == TAG_STROKE) { "unknown stroke tag $tag" }
        val preset = input.readInt()
        val argb = input.readInt()
        val scale = input.readFloat()
        val seed = input.readLong()
        val count = input.readInt()
        check(count in 0..MAX_SAMPLES) { "bad sample count $count" }
        val samples = FloatArray(count * STRIDE)
        val times = IntArray(count)
        for (i in 0 until count) {
            for (k in 0 until STRIDE) samples[i * STRIDE + k] = input.readFloat()
            times[i] = input.readInt()
        }
        return SavedStroke(preset, argb, scale, seed, samples, times, count)
    }
}

/** Page file: every stroke of one page, in drawing order. */
class StrokeFile(
    private val dir: File,
    private val pageId: String,
) {
    private val file: File get() = File(File(dir, PAGES_DIR), "$pageId.bin")

    /** Strokes already on disk. A torn or corrupt tail keeps everything before it. */
    suspend fun load(): List<SavedStroke> =
        withContext(Dispatchers.IO) {
            val target = file
            if (!target.isFile) return@withContext emptyList()
            val out = ArrayList<SavedStroke>()
            try {
                DataInputStream(BufferedInputStream(target.inputStream())).use { input ->
                    val magic = ByteArray(MAGIC.size)
                    input.readFully(magic)
                    if (!magic.contentEquals(MAGIC) || input.readUnsignedByte() != VERSION) {
                        return@withContext emptyList()
                    }
                    while (true) {
                        out += SavedStrokeCodec.read(input) ?: break
                    }
                }
            } catch (_: EOFException) {
                // Torn final record: keep the strokes before it.
            } catch (_: RuntimeException) {
                // Corrupt payload: keep the strokes before it rather than losing the page.
            }
            out
        }

    /** Adds one stroke; creates the file with its header when it does not exist yet. */
    suspend fun append(stroke: SavedStroke) =
        withContext(Dispatchers.IO) {
            file.parentFile?.mkdirs()
            val target = file
            val fresh = !target.isFile || target.length() == 0L
            DataOutputStream(BufferedOutputStream(FileOutputStream(target, true))).use { out ->
                if (fresh) writeHeader(out)
                SavedStrokeCodec.write(out, stroke)
            }
        }

    /** Replaces the whole page file (undo, clear, erase). */
    suspend fun rewrite(strokes: List<SavedStroke>) =
        withContext(Dispatchers.IO) {
            file.parentFile?.mkdirs()
            DataOutputStream(BufferedOutputStream(file.outputStream())).use { out ->
                writeHeader(out)
                strokes.forEach { SavedStrokeCodec.write(out, it) }
            }
        }

    private fun writeHeader(out: DataOutputStream) {
        out.write(MAGIC)
        out.writeByte(VERSION)
    }

    companion object {
        /** Directory holding one file per page. */
        const val PAGES_DIR = "pages"
        private const val VERSION = 1
        private val MAGIC = "PWST".toByteArray(Charsets.US_ASCII)
    }
}
