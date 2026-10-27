package dev.penwise.ink

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.io.File

/** Everything read from disk for one page, geometry rebuilt; ready to be swapped in without further waiting. */
internal class LoadedPage(
    val id: String,
    val file: StrokeFile,
    val undoFile: UndoHistoryFile,
    val strokes: List<SavedStroke>,
    val undo: List<HistoryStep>,
    val redo: List<HistoryStep>,
)

/**
 * Which page's files the view model currently writes to.
 *
 * [file] and [undoFile] are re-pointed only by [adopt], in the same synchronous step that swaps the store's
 * contents, so a save can never pair one page's strokes with another page's file. Loading is separate and touches
 * no shared state: a slow load of page B leaves page A fully writable until B is ready.
 */
internal class PageSession(
    private val dir: File,
    initial: String,
) {
    /** The page whose strokes the store holds; read from OkHttp's thread to route frames, hence volatile. */
    @Volatile
    var id: String = initial
        private set

    var file: StrokeFile = StrokeFile(dir, initial)
        private set

    var undoFile: UndoHistoryFile = UndoHistoryFile(dir, initial)
        private set

    /** Reads page `pageId` and its undo history; strokes the engine refuses to build are skipped, never fatal. */
    suspend fun load(
        pageId: String,
        build: (SavedStroke) -> Unit = { it.rebuild() },
        onSkip: SkipReport = StrokeRebuild.logSkip,
    ): LoadedPage {
        val pageFile = StrokeFile(dir, pageId)
        val history = UndoHistoryFile(dir, pageId)
        val saved = pageFile.load()
        val (undo, redo) = history.load()
        return withContext(Dispatchers.Default) {
            LoadedPage(
                id = pageId,
                file = pageFile,
                undoFile = history,
                strokes = StrokeRebuild.usable(saved, build, onSkip),
                undo = HistoryGeometry.usable(undo, build, onSkip),
                redo = HistoryGeometry.usable(redo, build, onSkip),
            )
        }
    }

    /** Makes `page` the open one. Not suspending on purpose: callers swap the store in the same step. */
    fun adopt(page: LoadedPage) {
        id = page.id
        file = page.file
        undoFile = page.undoFile
    }

    /** The open page was given a new id (the server minted one); its files have already been moved. */
    fun rename(to: String) {
        id = to
        file = StrokeFile(dir, to)
        undoFile = UndoHistoryFile(dir, to)
    }
}
