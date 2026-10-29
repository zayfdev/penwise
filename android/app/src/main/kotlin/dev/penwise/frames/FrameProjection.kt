package dev.penwise.frames

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

/**
 * The open page's frame items and the server's project list, projected into what [FrameLayer] opens: one [FrameSpec]
 * per frame whose project is known, with the URL on the host the tablet syncs with.
 *
 * A frame whose project is missing (the list has not arrived yet, or the project was deleted) opens nothing; a later
 * project list brings it in without an op. Updated from the UI thread and from the sync path alike.
 */
internal class FrameProjection {
    private val lock = Any()
    private var items: List<SceneFrame> = emptyList()
    private var projects: List<DesignProject> = emptyList()
    private var httpBase: String? = null
    private var token = ""

    private val _specs = MutableStateFlow<List<FrameSpec>>(emptyList())

    /** The frames to show: the open page's items that have a known project, in paint order. */
    val specs: StateFlow<List<FrameSpec>> = _specs.asStateFlow()

    private val _listed = MutableStateFlow<List<DesignProject>>(emptyList())

    /** The project list as last fetched (what a "new frame" picker offers). */
    val listed: StateFlow<List<DesignProject>> = _listed.asStateFlow()

    /** The open page's frames, or the one with [id]; the single source for a preset/route edit. */
    fun item(id: String): SceneFrame? = synchronized(lock) { items.firstOrNull { it.id == id } }

    fun updateItems(next: List<SceneFrame>) {
        synchronized(lock) { items = next }
        assemble()
    }

    fun updateProjects(next: List<DesignProject>) {
        synchronized(lock) {
            projects = next
            _listed.value = next
        }
        assemble()
    }

    /** The server the tablet currently syncs with ([httpBase], token), which decides every frame URL. */
    fun updateEndpoint(
        base: String?,
        token: String,
    ) {
        val changed =
            synchronized(lock) {
                val same = httpBase == base && this.token == token
                httpBase = base
                this.token = token
                !same
            }
        if (changed) assemble()
    }

    private fun assemble() {
        val next =
            synchronized(lock) {
                val byId = projects.associateBy { it.id }
                items.mapNotNull { item ->
                    val project = byId[item.project] ?: return@mapNotNull null
                    val url = FrameUrls.frameUrl(httpBase, project.port, item.route, token) ?: return@mapNotNull null
                    FrameSpec(
                        id = item.id,
                        project = item.project,
                        route = item.route,
                        preset = item.preset,
                        url = url,
                        x = item.x,
                        y = item.y,
                        cssWidth = item.cssWidth,
                        cssHeight = item.cssHeight,
                    )
                }
            }
        _specs.value = next
    }
}
