package dev.penwise

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.viewModels
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import dev.penwise.ink.InkCanvas
import dev.penwise.ink.PageStrip
import dev.penwise.ink.PageViewModel
import dev.penwise.tools.SyncDialog
import dev.penwise.tools.Toolbar

private const val TOOLBAR_MARGIN_DP = 12

class MainActivity : ComponentActivity() {
    private val page: PageViewModel by viewModels()

    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)
        val store = page.store
        val tools = page.tools
        page.loadPage()
        setContent {
            var showSync by remember { mutableStateOf(false) }
            val pages by page.pages.collectAsState()
            if (showSync) {
                SyncDialog(
                    initialUrl = page.syncUrl,
                    initialToken = page.syncToken,
                    onDismiss = { showSync = false },
                    onSave = { url, token ->
                        page.updateSync(url, token)
                        showSync = false
                    },
                )
            }
            Box(modifier = Modifier.fillMaxSize()) {
                InkCanvas(store = store, brushes = tools, liveOut = page.liveOut, modifier = Modifier.fillMaxSize())
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
                        onUndo = store::undo,
                        onRedo = store::redo,
                        onClear = store::clear,
                        onSync = { showSync = true },
                    )
                    PageStrip(
                        pages = pages,
                        activeId = pages.active,
                        onSelect = page::selectPage,
                        onNew = page::newPage,
                        onRename = page::renamePage,
                        onDelete = page::deletePage,
                    )
                }
            }
        }
    }
}
