package dev.penwise.tools

import android.content.Context
import android.net.Uri
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.window.Dialog
import dev.penwise.R
import dev.penwise.frames.ChromeStyle
import dev.penwise.ink.PageViewModel
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.io.OutputStream

private const val PANEL_COLOR = 0xFF23252A.toInt()
private const val TEXT_COLOR = 0xFFE6E8EC.toInt()
private const val ERROR_COLOR = 0xFFB3261E.toInt()
private const val PANEL_RADIUS_DP = 12
private const val PAD_DP = 16
private const val GAP_DP = 10
private const val TITLE_SP = 16
private const val BODY_SP = 14

/** A file the user picks (system "save as" picker); [write] fills it. Returns the function that opens the picker. */
@Composable
fun rememberFileExport(
    name: String,
    write: suspend (OutputStream) -> Unit,
): () -> Unit {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val picker =
        rememberLauncherForActivityResult(ActivityResultContracts.CreateDocument("application/octet-stream")) { uri ->
            if (uri != null) scope.launch { save(context, uri, write) }
        }
    return { picker.launch(name) }
}

private suspend fun save(
    context: Context,
    uri: Uri,
    write: suspend (OutputStream) -> Unit,
) {
    withContext(Dispatchers.IO) {
        context.contentResolver.openOutputStream(uri)?.use { write(it) }
    }
}

/**
 * What the sync layer must say out loud, over any screen: the state file cannot be written, an unreadable state
 * file was kept aside, the saved state could not be opened at all, or the server is a different one and the
 * tablet's ink needs a decision.
 */
@Composable
fun SyncNotices(page: PageViewModel) {
    val ui = page.syncUi
    val failing by ui.stateFailing.collectAsState()
    val recovered by ui.recovered.collectAsState()
    val startupError by ui.startupError.collectAsState()
    val changed by ui.serverChanged.collectAsState()
    val refused by ui.editRefused.collectAsState()
    val notice by ui.notice.collectAsState()
    // One answer per question: a second tap would ask the core to resolve a change it no longer holds.
    var answered by remember(changed) { mutableStateOf(false) }
    val exportState = rememberFileExport("penwise-state-backup.pwsb") { ui.exportState(it) }

    if (failing) Banner(stringResource(R.string.sync_state_failing), Alignment.TopCenter)
    if (refused != null) {
        Banner(stringResource(R.string.sync_edit_refused), Alignment.BottomCenter) { ui.editRefused.value = null }
    }
    notice?.let { message ->
        val text = stringResource(message) + " " + stringResource(R.string.notice_dismiss)
        Banner(text, Alignment.BottomCenter) { ui.notice.value = null }
    }
    if (changed) {
        NoticeDialog(
            title = stringResource(R.string.sync_server_changed_title),
            body = stringResource(R.string.sync_server_changed_body),
        ) {
            val keep = stringResource(R.string.sync_server_keep)
            ToolChip(keep, selected = true, onClick = {
                if (!answered) {
                    answered = true
                    page.resolveServerChange(keep = true)
                }
            })
            ToolChip(stringResource(R.string.sync_server_export), selected = false, onClick = exportState)
            val drop = stringResource(R.string.sync_server_drop)
            ToolChip(drop, selected = false, onClick = {
                if (!answered) {
                    answered = true
                    page.resolveServerChange(keep = false)
                }
            })
        }
    } else if (startupError != null) {
        NoticeDialog(
            stringResource(R.string.sync_startup_error_title),
            stringResource(R.string.sync_startup_error_body, startupError.orEmpty()),
        ) {}
    } else if (recovered.isNotEmpty()) {
        NoticeDialog(
            stringResource(R.string.sync_recovery_title),
            stringResource(R.string.sync_recovery_body, recovered.joinToString()),
        ) {
            ToolChip(stringResource(R.string.ok), selected = true, onClick = ui::dismissRecovery)
        }
    }
}

/** A red strip over the page; tapping it runs [onDismiss] (none: it stays until the cause is gone). */
@Composable
private fun Banner(
    text: String,
    alignment: Alignment,
    onDismiss: (() -> Unit)? = null,
) {
    Box(Modifier.safeDrawingPadding().fillMaxWidth(), contentAlignment = alignment) {
        BasicText(
            text,
            modifier =
                Modifier
                    .background(Color(ERROR_COLOR), RoundedCornerShape(GAP_DP.dp))
                    .then(if (onDismiss != null) Modifier.clickable(onClick = onDismiss) else Modifier)
                    .padding(GAP_DP.dp),
            style = TextStyle(fontFamily = ChromeStyle.font, fontSize = BODY_SP.sp, color = Color.White),
        )
    }
}

@Composable
private fun NoticeDialog(
    title: String,
    body: String,
    buttons: @Composable () -> Unit,
) {
    Dialog(onDismissRequest = {}) {
        Column(
            Modifier
                .background(Color(PANEL_COLOR), RoundedCornerShape(PANEL_RADIUS_DP.dp))
                .padding(PAD_DP.dp),
            verticalArrangement = Arrangement.spacedBy(GAP_DP.dp),
        ) {
            BasicText(
                title,
                style = TextStyle(fontFamily = ChromeStyle.font, fontSize = TITLE_SP.sp, color = Color(TEXT_COLOR)),
            )
            BasicText(
                body,
                style = TextStyle(fontFamily = ChromeStyle.font, fontSize = BODY_SP.sp, color = Color(TEXT_COLOR)),
            )
            Row(horizontalArrangement = Arrangement.spacedBy(GAP_DP.dp)) { buttons() }
        }
    }
}
