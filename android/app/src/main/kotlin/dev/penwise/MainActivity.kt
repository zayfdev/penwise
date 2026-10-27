package dev.penwise

import android.content.Intent
import android.os.Bundle
import android.util.Log
import androidx.activity.ComponentActivity
import androidx.activity.compose.BackHandler
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.viewModels
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import dev.penwise.home.HomeActions
import dev.penwise.home.HomeScreen
import dev.penwise.ink.InkCanvas
import dev.penwise.ink.PageList
import dev.penwise.ink.PageStrip
import dev.penwise.ink.PageStyle
import dev.penwise.ink.PageViewModel
import dev.penwise.ink.SyncService
import dev.penwise.sync.SyncStatus
import dev.penwise.tools.SyncDialog
import dev.penwise.tools.Toolbar

private const val TOOLBAR_MARGIN_DP = 12

class MainActivity : ComponentActivity() {
    private val page: PageViewModel by viewModels()

    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)
        SyncService.start(this)
        page.loadPage()
        // Debug-only end-to-end replay: `am start … --es fixture <path>`.
        intent?.getStringExtra("fixture")?.let { path ->
            Log.i("PenwiseFixture", "replay requested: $path")
            page.requestReplay(path)
        }
        handlePairing(intent)
        setContent {
            val pages by page.pages.collectAsState()
            val status by page.status.collectAsState()
            var showSync by remember { mutableStateOf(false) }
            var openPage by remember { mutableStateOf<String?>(null) }

            if (showSync) {
                SyncDialog(
                    initialUrl = page.syncUrl,
                    initialToken = page.syncToken,
                    status = syncStatusText(status, page.pendingOps()),
                    onDismiss = { showSync = false },
                    onSave = { url, token ->
                        page.updateSync(url, token)
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
                            page.newPage(title, template, notebook)?.let { openPage = it }
                        },
                        newNotebook = page::createNotebook,
                        renameNotebook = page::renameNotebook,
                        deleteNotebook = page::deleteNotebook,
                        renamePage = page::renamePage,
                        deletePage = page::deletePage,
                        sync = { showSync = true },
                    )
                }

            BackHandler(enabled = openPage != null) { openPage = null }

            if (openPage == null) {
                HomeScreen(pages = pages, actions = actions, modifier = Modifier.fillMaxSize())
            } else {
                CanvasScreen(
                    pages = pages,
                    page = page,
                    onHome = { openPage = null },
                    onSync = { showSync = true },
                )
            }
        }
    }

    /** `penwise://pair?code=…&url=…`: exchange the code for a token and point sync at that server. */
    private fun handlePairing(intent: Intent?) {
        val data = intent?.data ?: return
        if (data.scheme != "penwise" || data.host != "pair") return
        val code = data.getQueryParameter("code") ?: return
        val url = data.getQueryParameter("url").orEmpty()
        Log.i("PenwisePair", "pairing with code from ${url.ifBlank { "settings" }}")
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

@Composable
private fun syncStatusText(
    status: SyncStatus,
    pending: Int,
): String {
    val label =
        stringResource(
            when (status) {
                SyncStatus.ONLINE -> R.string.sync_status_online
                SyncStatus.CONNECTING -> R.string.sync_status_connecting
                SyncStatus.BAD_TOKEN -> R.string.sync_status_bad_token
                SyncStatus.STOPPED -> R.string.sync_status_stopped
            },
        )
    return if (pending > 0) stringResource(R.string.sync_status_pending, label, pending) else label
}

@Composable
private fun CanvasScreen(
    pages: PageList,
    page: PageViewModel,
    onHome: () -> Unit,
    onSync: () -> Unit,
) {
    val tools = page.tools
    val scene by page.scene.collectAsState()
    val open = scene ?: return
    Box(modifier = Modifier.fillMaxSize()) {
        InkCanvas(
            scene = open,
            brushes = tools,
            style = PageStyle.of(pages.page(pages.active)),
            images = page.images,
            liveOut = page.liveOut,
            modifier = Modifier.fillMaxSize(),
        )
        Column(
            modifier =
                Modifier
                    .align(Alignment.TopCenter)
                    .safeDrawingPadding()
                    .padding(TOOLBAR_MARGIN_DP.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(TOOLBAR_MARGIN_DP.dp),
        ) {
            Toolbar(
                state = tools,
                onUndo = { open.undo() },
                onRedo = { open.redo() },
                onClear = { open.clearPage() },
                onSync = onSync,
                onHome = onHome,
            )
            PageStrip(
                pages = pages,
                activeId = pages.active,
                onSelect = page::selectPage,
                onNew = { title, template -> page.newPage(title, template) },
                onRename = page::renamePage,
                onDelete = page::deletePage,
            )
        }
    }
}
