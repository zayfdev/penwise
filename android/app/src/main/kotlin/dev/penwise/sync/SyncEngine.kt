package dev.penwise.sync

import java.io.File

/**
 * The whole sync layer of the app: the [SyncCore], its durable state file, the `/sync` connection and the page
 * catalog. One instance per process lifetime of the view model.
 *
 * Every local change (an edit blob, a catalog op) and every applied server frame requests a state write and a
 * push, so the outbox survives process death and reaches the server as soon as a socket is up.
 */
class SyncEngine(
    filesDir: File,
    endpoints: () -> List<String>,
    token: () -> String,
    listener: SyncListener,
    clock: () -> Long = System::currentTimeMillis,
    network: NetworkSource? = null,
) : AutoCloseable {
    private val stateDir = File(filesDir, "sync")
    private val stateFile = SyncStateFile(stateDir)

    /** The core; its `lock` guards every native call, `PageScene`s included. */
    val core: SyncCore = stateFile.open(clock)

    /** State files this start could not read, kept aside; the user must be told (see [SyncStateFile.open]). */
    val recovered: List<File> = stateFile.recovered
    val library = Library(core)
    val legacy = LegacyImport(filesDir, stateDir)
    private val persister = StatePersister(core, stateFile, onFailing = listener::onStateFailing)
    private val announce = Announcer(core)
    private val connection: SyncConnection =
        SyncConnection(
            core,
            endpoints,
            token,
            clock = clock,
            activeNetwork = { network?.active() },
            listener =
                object : SyncListener by listener {
                    override fun onStatus(status: SyncStatus) {
                        if (status == SyncStatus.ONLINE) announce.resend(::send)
                        listener.onStatus(status)
                    }
                },
        )

    /** Reconnects at once when the network returns; closed with the engine. */
    private val networkWatch: AutoCloseable? = network?.watch(connection::networkValidated)

    init {
        core.onMutated = {
            persister.request()
            connection.kick()
        }
    }

    /** Connection state. */
    val status: SyncStatus get() = connection.status

    /** The endpoint the socket is on (the one connected, when [status] is online); `null` before the first dial. */
    val connectedUrl: String? get() = connection.currentUrl

    /** Connects (again, after a settings change: [forgetGood] starts over from the first endpoint). */
    fun start(forgetGood: Boolean = false) {
        connection.start(forgetGood)
    }

    /** Tells other devices which page the tablet shows (the browser follows). */
    fun showing(page: String) {
        announce.page = page
        announce.resend(::send)
    }

    /** Sends a droppable frame (live preview); ignored while offline. */
    fun send(frame: ByteArray) {
        connection.sendLive(frame)
    }

    /** Writes the state file now and waits for that write; true when the state is on disk. */
    fun flush(): Boolean = persister.flush()

    /** True while writing the state file keeps failing. */
    val stateFailing: Boolean get() = persister.failing

    /**
     * Answers the "this is a different server" question (see [SyncCore.resolveServerChange]) and reconnects, so the
     * new server gets the handshake again with the decided pin.
     */
    fun resolveServerChange(keep: Boolean): Outcome {
        val outcome = core.resolveServerChange(keep)
        connection.start()
        return outcome
    }

    /** Persists the outbox, stops the socket and frees the core. Scenes opened on it must be closed first. */
    override fun close() {
        networkWatch?.close()
        connection.stop()
        persister.close()
        core.close()
    }
}

/** Remembers the page to announce so a reconnect repeats it. */
private class Announcer(
    private val core: SyncCore,
) {
    @Volatile
    var page: String? = null

    fun resend(send: (ByteArray) -> Unit) {
        val bytes = page?.let(Ids::bytes) ?: return
        send(core.switchPage(bytes))
    }
}
