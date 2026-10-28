package dev.penwise

import android.content.Intent
import android.os.Bundle
import android.util.Log
import androidx.activity.ComponentActivity
import androidx.activity.compose.BackHandler
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.viewModels
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.ProduceStateScope
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import dev.penwise.home.HomeActions
import dev.penwise.home.HomeScreen
import dev.penwise.ink.PageViewModel
import dev.penwise.ink.SyncInfo
import dev.penwise.ink.SyncService
import dev.penwise.sync.SyncStatus
import dev.penwise.tools.SyncDialog
import dev.penwise.tools.SyncNotices
import dev.penwise.tools.rememberFileExport
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

private const val INFO_REFRESH_MS = 1_000L
private const val PENDING_REFRESH_MS = 2_000L

class MainActivity : ComponentActivity() {
    private val page: PageViewModel by viewModels()

    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)
        SyncService.start(this)
        page.loadPage()
        // A restored activity still holds the launch intent, whose pairing code is spent and fixture already ran.
        if (savedInstanceState == null) handleLaunch(intent)
        setContent {
            val pages by page.pages.collectAsState()
            val status by page.status.collectAsState()
            val held by page.syncUi.serverChanged.collectAsState()
            val scope = rememberCoroutineScope()
            var showSync by rememberSaveable { mutableStateOf(false) }
            var openPage by rememberSaveable { mutableStateOf<String?>(null) }
            // Kept here, not in CanvasScreen, so the drawer stays as the user left it across page switches.
            var drawerOpen by rememberSaveable { mutableStateOf(false) }

            SyncNotices(page)
            val info by produceState(SyncInfo(), showSync) {
                // Read off the UI thread (it takes the core lock); refreshed while the dialog is open.
                while (showSync) {
                    value = page.syncUi.info()
                    delay(INFO_REFRESH_MS)
                }
            }
            // The dot on the page's bar: edits waiting to send, refreshed while a page is open.
            val waiting by produceState(0, openPage != null) { pollPending(page, openPage != null) }
            val exportRefused = rememberFileExport("penwise-refused-ops.pwdl") { page.syncUi.exportDeadLetters(it) }

            if (showSync) {
                SyncDialog(
                    initialUrl = page.syncUrl,
                    initialToken = page.syncToken,
                    status = syncStatusText(status, held, info.pending, info.deadLetters.size),
                    deadLetters = info.deadLetters,
                    onExportDeadLetters = exportRefused,
                    onClearDeadLetters = { scope.launch { page.syncUi.clearDeadLetters() } },
                    onDismiss = { showSync = false },
                    onSave = { url, token ->
                        // The field is prefilled with the address in use; saving it unchanged must not pin it.
                        page.updateSync(if (url == page.syncUrl) null else url, token)
                        showSync = false
                    },
                )
            }

            val actions =
                remember(pages.active) {
                    HomeActions(
                        openPage = { id ->
                            page.selectPage(id)
                            openPage = id
                        },
                        newPage = { title, template, notebook ->
                            page.catalog.newPage(title, template, notebook) { openPage = it }
                        },
                        newNotebook = page.catalog::createNotebook,
                        renameNotebook = page.catalog::renameNotebook,
                        deleteNotebook = page.catalog::deleteNotebook,
                        renamePage = page.catalog::renamePage,
                        deletePage = page.catalog::deletePage,
                        sync = { showSync = true },
                    )
                }

            // Back never leaves the app from a page: it closes the drawer, then returns to the home screen.
            BackHandler(enabled = openPage != null) {
                // A palm on the edge while writing is a Back gesture too: not while the pen is on or near the page.
                if (page.pen.active()) return@BackHandler
                if (drawerOpen) drawerOpen = false else openPage = null
            }

            if (openPage == null) {
                HomeScreen(pages = pages, actions = actions, modifier = Modifier.fillMaxSize())
            } else {
                CanvasScreen(
                    pages = pages,
                    page = page,
                    drawerOpen = drawerOpen,
                    onDrawerOpenChange = { drawerOpen = it },
                    onHome = { openPage = null },
                    onSync = { showSync = true },
                    syncStatus = status,
                    pending = waiting,
                )
            }
        }
    }

    private fun handleLaunch(intent: Intent?) {
        // Debug-only end-to-end replay: `am start … --es fixture <path>`.
        intent?.getStringExtra("fixture")?.let { path ->
            Log.i("PenwiseFixture", "replay requested: $path")
            page.requestReplay(path)
        }
        handlePairing(intent)
    }

    /** `penwise://pair?code=…&url=…`: exchange the code for a token and point sync at that server. */
    private fun handlePairing(intent: Intent?) {
        val data = intent?.data ?: return
        if (data.scheme != "penwise" || data.host != "pair") return
        val code = data.getQueryParameter("code") ?: return
        val url = data.getQueryParameter("url")?.takeIf { it.isNotBlank() }
        // Consume the link: the code works once, and nothing may replay it.
        intent.data = null
        Log.i("PenwisePair", "pairing with code from ${url ?: "settings"}")
        page.updateSync(url, code)
    }

    override fun onStop() {
        page.flushState()
        super.onStop()
    }

    override fun onDestroy() {
        SyncService.stop(this)
        super.onDestroy()
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        handlePairing(intent)
    }
}

private suspend fun ProduceStateScope<Int>.pollPending(
    page: PageViewModel,
    open: Boolean,
) {
    while (open) {
        value = page.syncUi.pending()
        delay(PENDING_REFRESH_MS)
    }
}

@Composable
private fun syncStatusText(
    status: SyncStatus,
    held: Boolean,
    pending: Int,
    refused: Int,
): String {
    val label =
        stringResource(
            when (status) {
                // Online, but nothing is applied or sent until the user answers the "different server" question.
                SyncStatus.ONLINE -> if (held) R.string.sync_status_held else R.string.sync_status_online

                SyncStatus.CONNECTING -> R.string.sync_status_connecting

                SyncStatus.BAD_TOKEN -> R.string.sync_status_bad_token

                SyncStatus.STOPPED -> R.string.sync_status_stopped
            },
        )
    val withPending = if (pending > 0) stringResource(R.string.sync_status_pending, label, pending) else label
    return if (refused > 0) stringResource(R.string.sync_status_refused, withPending, refused) else withPending
}
