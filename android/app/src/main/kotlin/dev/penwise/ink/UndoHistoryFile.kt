package dev.penwise.ink

import kotlinx.coroutines.withContext
import java.io.BufferedInputStream
import java.io.DataInputStream
import java.io.DataOutputStream
import java.io.EOFException
import java.io.File

/** One undoable step: strokes appended, removed (with their positions), or swapped for fragments. */
sealed interface HistoryStep {
    /** A stroke was added; undo removes it. */
    data class Added(
        val stroke: SavedStroke,
    ) : HistoryStep

    /** Strokes were removed; undo reinserts them at their original indices. */
    data class Removed(
        val removed: List<Pair<Int, SavedStroke>>,
    ) : HistoryStep

    /** A precise erase: strokes swapped for the fragments they left behind. */
    data class Replaced(
        val removed: List<SavedStroke>,
        val added: List<SavedStroke>,
    ) : HistoryStep
}

/**
 * On-disk undo history for one page: the same binary stroke records the page file uses, wrapped in step frames.
 *
 * Written after every history change and read when the page opens, so relaunching the app does not throw away
 * your last hundred edits. A torn tail keeps the steps before it.
 */
class UndoHistoryFile(
    private val dir: File,
    private val pageId: String,
) {
    private val file: File get() = File(File(dir, StrokeFile.PAGES_DIR), "$pageId.undo")

    /** Steps already on disk: `(undo, redo)`, oldest first. */
    suspend fun load(): Pair<List<HistoryStep>, List<HistoryStep>> =
        withContext(PageIo) {
            if (!file.isFile) return@withContext emptyList<HistoryStep>() to emptyList()
            try {
                DataInputStream(BufferedInputStream(file.inputStream())).use { input ->
                    val magic = ByteArray(MAGIC.size)
                    input.readFully(magic)
                    if (!magic.contentEquals(MAGIC) || input.readUnsignedByte() != VERSION) {
                        return@withContext emptyList<HistoryStep>() to emptyList()
                    }
                    val undo = readSteps(input)
                    val redo = readSteps(input)
                    undo to redo
                }
            } catch (_: EOFException) {
                emptyList<HistoryStep>() to emptyList()
            } catch (_: RuntimeException) {
                emptyList<HistoryStep>() to emptyList()
            }
        }

    /** Replaces the stored history; failures leave the previous file readable. */
    suspend fun save(
        undo: List<HistoryStep>,
        redo: List<HistoryStep>,
    ) = withContext(PageIo) {
        if (undo.isEmpty() && redo.isEmpty()) {
            file.delete()
            return@withContext
        }
        writeAtomically(file) { out ->
            out.write(MAGIC)
            out.writeByte(VERSION)
            writeSteps(out, undo)
            writeSteps(out, redo)
        }
    }

    private fun writeSteps(
        out: DataOutputStream,
        steps: List<HistoryStep>,
    ) {
        out.writeInt(steps.size)
        steps.forEach { step ->
            when (step) {
                is HistoryStep.Added -> {
                    out.writeByte(KIND_ADDED)
                    SavedStrokeCodec.write(out, step.stroke)
                }

                is HistoryStep.Removed -> {
                    out.writeByte(KIND_REMOVED)
                    out.writeInt(step.removed.size)
                    step.removed.forEach { (index, stroke) ->
                        out.writeInt(index)
                        SavedStrokeCodec.write(out, stroke)
                    }
                }

                is HistoryStep.Replaced -> {
                    out.writeByte(KIND_REPLACED)
                    out.writeInt(step.removed.size)
                    step.removed.forEach { SavedStrokeCodec.write(out, it) }
                    out.writeInt(step.added.size)
                    step.added.forEach { SavedStrokeCodec.write(out, it) }
                }
            }
        }
    }

    /** Reads one stack; a truncated or unknown record ends the stack instead of failing the load. */
    private fun readSteps(input: DataInputStream): List<HistoryStep> {
        val count = input.readInt()
        if (count < 0 || count > MAX_STEPS) return emptyList()
        val steps = ArrayList<HistoryStep>(count)
        for (index in 0 until count) {
            steps += readStep(input) ?: break
        }
        return steps
    }

    private fun readStep(input: DataInputStream): HistoryStep? =
        when (input.readUnsignedByte()) {
            KIND_ADDED -> {
                SavedStrokeCodec.read(input)?.let { HistoryStep.Added(it) }
            }

            KIND_REMOVED -> {
                readRemoved(input)
            }

            KIND_REPLACED -> {
                val removed = readStrokes(input)
                val added = readStrokes(input)
                if (removed == null || added == null) null else HistoryStep.Replaced(removed, added)
            }

            else -> {
                null
            }
        }

    private fun readRemoved(input: DataInputStream): HistoryStep? {
        val size = input.readInt()
        if (size < 0 || size > MAX_STROKES) return null
        val removed = ArrayList<Pair<Int, SavedStroke>>(size)
        repeat(size) {
            val index = input.readInt()
            val stroke = SavedStrokeCodec.read(input) ?: return null
            removed += index to stroke
        }
        return HistoryStep.Removed(removed)
    }

    private fun readStrokes(input: DataInputStream): List<SavedStroke>? {
        val size = input.readInt()
        if (size < 0 || size > MAX_STROKES) return null
        val strokes = ArrayList<SavedStroke>(size)
        repeat(size) {
            strokes += SavedStrokeCodec.read(input) ?: return null
        }
        return strokes
    }

    private companion object {
        const val VERSION = 1
        const val KIND_ADDED = 0
        const val KIND_REMOVED = 1
        const val KIND_REPLACED = 2
        const val MAX_STEPS = 500
        const val MAX_STROKES = 20_000
        val MAGIC = "PWUN".toByteArray(Charsets.US_ASCII)
    }
}
