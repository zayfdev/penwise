package dev.penwise.overview

import dev.penwise.frames.FrameHttp
import dev.penwise.frames.FrameServer
import dev.penwise.frames.LaneKey
import dev.penwise.sync.Ids
import org.json.JSONException
import org.json.JSONObject
import java.net.URLEncoder

private const val HTTP_BAD_REQUEST = 400
private const val HTTP_NOT_FOUND = 404

private fun encode(value: String): String = URLEncoder.encode(value, "UTF-8")

/** `GET /projects/{p}/history?route=&preset=` (contract §2.2). Blocking; callers run it off the main thread. */
internal object HistoryClient {
    /** The lane's generations, newest first; null when the server is away or answered something else. */
    fun fetch(
        server: FrameServer,
        lane: LaneKey,
    ): List<Generation>? {
        val path = "/projects/${encode(lane.project)}/history?route=${encode(lane.route)}&preset=${encode(lane.preset)}"
        val reply = FrameHttp(server).get(path)?.takeIf { it.ok } ?: return null
        return HistoryJson.parse(reply.body.decodeToString())
    }
}

/** What a `PUT .../strip-pages/{page}` came back with. */
internal enum class AttachResult {
    /** The page is on the strip. */
    OK,

    /** `404`: the server does not know the page yet (its creation has not synced); worth another try. */
    NOT_FOUND,

    /** `400`: the server will never take this page (outside the board's notebook, the board itself). */
    REJECTED,

    /** The server was away or failed; try again later. */
    FAILED,
}

/** `GET /projects/{p}/strip-pages` and `PUT /projects/{p}/strip-pages/{page}` (contract §2.3). Blocking. */
internal object StripPagesClient {
    /** The project's attached pages in attach order (pages as 32-hex ids); null when the server is away. */
    fun list(
        server: FrameServer,
        project: String,
    ): List<StripPage>? {
        val reply = FrameHttp(server).get("/projects/${encode(project)}/strip-pages")?.takeIf { it.ok } ?: return null
        return StripPagesJson.parse(reply.body.decodeToString())
    }

    /** Attaches the page [hexPage] (the tablet's 32-hex id, sent as the server's ULID) to [lane]'s strip. */
    fun attach(
        server: FrameServer,
        lane: LaneKey,
        hexPage: String,
    ): AttachResult {
        val ulid = Ids.ulid(hexPage) ?: return AttachResult.REJECTED
        val body = JSONObject().put("route", lane.route).put("preset", lane.preset).toString()
        val reply = FrameHttp(server).put("/projects/${encode(lane.project)}/strip-pages/$ulid", body)
        return when {
            reply == null -> AttachResult.FAILED
            reply.ok -> AttachResult.OK
            reply.code == HTTP_NOT_FOUND -> AttachResult.NOT_FOUND
            reply.code == HTTP_BAD_REQUEST -> AttachResult.REJECTED
            else -> AttachResult.FAILED
        }
    }
}

/** `GET /projects/{p}/rounds`: the round Send would close next. */
internal object OpenRoundJson {
    /** The number of the round whose status is `open`; null when the reply names none. */
    fun parse(json: String): Int? =
        try {
            val rounds = JSONObject(json).optJSONArray("rounds")
            (0 until (rounds?.length() ?: 0))
                .mapNotNull { rounds?.optJSONObject(it) }
                .lastOrNull { it.optString("status") == "open" }
                ?.optInt("n")
                ?.takeIf { it > 0 }
        } catch (ignored: JSONException) {
            null
        }

    fun fetch(
        server: FrameServer,
        project: String,
    ): Int? {
        val reply = FrameHttp(server).get("/projects/${encode(project)}/rounds")?.takeIf { it.ok } ?: return null
        return parse(reply.body.decodeToString())
    }
}

/** What the server keeps for the strips: lane history, attached pages and each project's open round. */
internal data class OverviewRemote(
    val history: Map<LaneKey, List<Generation>> = emptyMap(),
    val stripPages: Map<LaneKey, List<StripPage>> = emptyMap(),
    /** Project to the round that is open now. */
    val openRounds: Map<String, Int> = emptyMap(),
) {
    companion object {
        /** Fetches [lanes]' history and their projects' strip pages and open round; what fails is left out. */
        fun fetch(
            server: FrameServer,
            lanes: List<LaneKey>,
        ): OverviewRemote {
            val history = lanes.mapNotNull { lane -> HistoryClient.fetch(server, lane)?.let { lane to it } }.toMap()
            val projects = lanes.map { it.project }.distinct()
            val pages =
                projects.flatMap { project ->
                    StripPagesClient.list(server, project).orEmpty().map {
                        project to
                            it
                    }
                }
            val byLane =
                lanes.associateWith { lane ->
                    pages
                        .filter { (project, page) ->
                            project == lane.project && page.route == lane.route &&
                                page.preset == lane.preset
                        }.map { it.second }
                }
            val rounds =
                projects
                    .mapNotNull { project ->
                        OpenRoundJson.fetch(server, project)?.let { project to it }
                    }.toMap()
            return OverviewRemote(history, byLane, rounds)
        }
    }
}
