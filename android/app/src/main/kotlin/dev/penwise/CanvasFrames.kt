package dev.penwise

import android.widget.Toast
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.ui.platform.LocalContext
import dev.penwise.frames.FrameLockActions
import dev.penwise.frames.FrameOverlayModel
import dev.penwise.frames.FrameStageActions
import dev.penwise.frames.FrameStageLive
import dev.penwise.frames.FrameStageModel
import dev.penwise.ink.PageScene
import dev.penwise.ink.PageViewModel
import dev.penwise.sync.SyncStatus
import kotlinx.coroutines.flow.Flow

/** The frame stage's inputs for the open page: the frames, marks and server the model holds, and the calls back. */
@Composable
internal fun rememberFrameStageModel(
    page: PageViewModel,
    open: PageScene,
): FrameStageModel {
    val session = page.frameSession
    val specs by page.frames.collectAsState()
    val marks by page.marks.collectAsState()
    val watchKeys by page.watchKeys.collectAsState()
    val server by session.server.collectAsState()
    ToastNotices(session.notices)
    val actions =
        FrameStageActions(
            onPreset = session::setFramePreset,
            onRoute = session::setFrameRoute,
            lock =
                FrameLockActions(
                    onChoose = { session.setFrameChosen(it, true) },
                    onOpenOverview = page::openOverview,
                ),
            onLayout = session::setLayout,
            onMarkStatus = session::setMarkStatus,
            onFocused = session::focused,
            onUserFocus = page::frameFocused,
        )
    return FrameStageModel(
        specs,
        marks,
        watchKeys,
        actions,
        FrameStageLive(server, open, session.windows, page.frameControl, page.pen),
    )
}

/**
 * The chip area over the canvas: the Send button (when the page has frames) and the held agent move. It belongs to the
 * screen, not to the frame stage, because the agent may ask for a move on a page that has no frames yet.
 */
@Composable
internal fun rememberFrameOverlay(page: PageViewModel): FrameOverlayModel {
    val specs by page.frames.collectAsState()
    val chip by page.agent.chip.collectAsState()
    val online by page.status.collectAsState()
    return FrameOverlayModel(
        showSend = specs.isNotEmpty(),
        canSend = online == SyncStatus.ONLINE,
        chip = chip,
        onSend = page.frameSession::sendRound,
        onChipTap = page.agent::accept,
        onChipDismiss = page.agent::dismiss,
    )
}

/** Shows the messages the frames' calls raise (a round sent, nothing to send) as brief toasts. */
@Composable
private fun ToastNotices(notices: Flow<Int>) {
    val context = LocalContext.current
    LaunchedEffect(notices) {
        notices.collect { Toast.makeText(context, it, Toast.LENGTH_SHORT).show() }
    }
}
