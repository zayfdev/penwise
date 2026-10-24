package dev.penwise.ink

import org.json.JSONObject
import java.io.File

/**
 * The page library as the view model sees it: local cache first, server second.
 *
 * Every server call is best-effort (the tablet is offline-first), so nothing here throws or waits: mutations land
 * on disk immediately and reach the server when it is reachable.
 */
class PageLibrary(
    dir: File,
    private val settings: SyncSettings,
) {
    private val store = PageStore(dir)

    /** The cached index, or a fresh single-page one. */
    fun load(): PageList = store.load()

    /** Writes the index to disk. */
    fun save(list: PageList) = store.save(list)

    /** Stroke storage for one page. */
    fun strokes(pageId: String): StrokeFile = store.strokes(pageId)

    /** A fresh page id in the shape the server accepts from clients. */
    fun newPageId(): String = store.newPageId()

    /** Body for `POST /pages`, including the id so an offline-created page keeps its identity. */
    fun createBody(page: PageMeta): String {
        val kind =
            if (page.infinite) {
                JSONObject().put("t", "infinite")
            } else {
                JSONObject()
                    .put("t", "fixed")
                    .put("width", PageListCodec.A4_WIDTH)
                    .put("height", PageListCodec.A4_HEIGHT)
            }
        return JSONObject()
            .put("id", page.id)
            .put("title", page.title)
            .put("background", page.background)
            .put("kind", kind)
            .toString()
    }

    /** Tells the server about a new page; `null` when it is unreachable. */
    fun createRemote(page: PageMeta): String? = client().createPage(createBody(page))

    /** Renames a page on the server. */
    fun renameRemote(
        id: String,
        title: String,
    ): String? = client().updatePage(id, JSONObject().put("title", title).toString())

    /** Deletes a page on the server. */
    fun deleteRemote(id: String): String? = client().deletePage(id)

    private fun client(): LibraryClient = LibraryClient(settings.endpoints().firstOrNull().orEmpty(), settings.token)
}
