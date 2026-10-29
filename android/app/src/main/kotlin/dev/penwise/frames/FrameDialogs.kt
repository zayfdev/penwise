package dev.penwise.frames

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicText
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.window.Dialog
import dev.penwise.R
import dev.penwise.tools.PanelButton
import dev.penwise.tools.ToolChip

private const val PANEL = 0xFF23252A.toInt()
private const val PANEL_TEXT = 0xFFE8EAEE.toInt()
private const val PANEL_MUTED = 0xFF9AA0AA.toInt()
private const val FIELD = 0x33FFFFFF
private const val MAX_WIDTH_DP = 420
private const val PAD_DP = 18
private const val GAP_DP = 10
private const val SMALL_GAP_DP = 4
private const val TITLE_SP = 16
private const val LABEL_SP = 11
private const val TEXT_SP = 14

/**
 * Adds a frame to the page: pick a linked project, type the route (default `/`) and the size preset. The caller places
 * it at the centre of the visible page.
 */
@Composable
internal fun AddFrameDialog(
    projects: List<DesignProject>,
    onDismiss: () -> Unit,
    onAdd: (project: String, route: String, preset: String) -> Unit,
) {
    var project by rememberSaveable { mutableStateOf(projects.firstOrNull()?.id.orEmpty()) }
    var route by rememberSaveable { mutableStateOf(FrameRoutes.ROOT) }
    var preset by rememberSaveable { mutableStateOf(FramePreset.PHONE.key) }
    // The list arrives after the dialog opens when the link was never fetched on this launch.
    LaunchedEffect(projects) {
        if (projects.none { it.id == project }) project = projects.firstOrNull()?.id.orEmpty()
    }
    val normalized = FrameRoutes.normalize(route)
    Dialog(onDismissRequest = onDismiss) {
        Column(
            Modifier
                .widthIn(max = MAX_WIDTH_DP.dp)
                .background(Color(PANEL), RoundedCornerShape(12.dp))
                .padding(PAD_DP.dp),
            verticalArrangement = Arrangement.spacedBy(GAP_DP.dp),
        ) {
            Title(stringResource(R.string.frame_add_title))
            if (projects.isEmpty()) {
                BasicText(
                    stringResource(R.string.frame_no_projects),
                    style = TextStyle(fontSize = TEXT_SP.sp, color = Color(PANEL_MUTED)),
                )
            } else {
                Label(stringResource(R.string.frame_project))
                ProjectChoices(projects, project) { project = it }
                Label(stringResource(R.string.frame_route))
                RouteField(route) { route = it }
                Label(stringResource(R.string.frame_preset))
                PresetChoices(preset) { preset = it }
            }
            Row(horizontalArrangement = Arrangement.spacedBy(GAP_DP.dp)) {
                if (projects.isNotEmpty()) {
                    PanelButton(
                        stringResource(R.string.frame_add),
                        primary = true,
                        onClick = { normalized?.let { onAdd(project, it, preset) } },
                    )
                }
                PanelButton(stringResource(R.string.cancel), primary = false, onClick = onDismiss)
            }
        }
    }
}

/** Typing a route for a frame: the page inside loads it (another route of the same project). */
@Composable
internal fun RouteDialog(
    current: String,
    onDismiss: () -> Unit,
    onOpen: (String) -> Unit,
) {
    var route by rememberSaveable { mutableStateOf(current) }
    val normalized = FrameRoutes.normalize(route)
    Dialog(onDismissRequest = onDismiss) {
        Column(
            Modifier
                .widthIn(max = MAX_WIDTH_DP.dp)
                .background(Color(PANEL), RoundedCornerShape(12.dp))
                .padding(PAD_DP.dp),
            verticalArrangement = Arrangement.spacedBy(GAP_DP.dp),
        ) {
            Title(stringResource(R.string.frame_route_title))
            RouteField(route) { route = it }
            Row(horizontalArrangement = Arrangement.spacedBy(GAP_DP.dp)) {
                PanelButton(
                    stringResource(R.string.frame_open),
                    primary = true,
                    onClick = { normalized?.let(onOpen) },
                )
                PanelButton(stringResource(R.string.cancel), primary = false, onClick = onDismiss)
            }
        }
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun ProjectChoices(
    projects: List<DesignProject>,
    selected: String,
    onSelect: (String) -> Unit,
) {
    FlowRow(horizontalArrangement = Arrangement.spacedBy(SMALL_GAP_DP.dp)) {
        for (linked in projects) {
            ToolChip(linked.name, selected = linked.id == selected, onClick = { onSelect(linked.id) })
        }
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun PresetChoices(
    selected: String,
    onSelect: (String) -> Unit,
) {
    FlowRow(horizontalArrangement = Arrangement.spacedBy(SMALL_GAP_DP.dp)) {
        for (choice in FramePreset.entries) {
            ToolChip(
                "${choice.key} · ${choice.cssWidth}×${choice.cssHeight}",
                selected = choice.key == selected,
                onClick = { onSelect(choice.key) },
            )
        }
    }
}

@Composable
private fun RouteField(
    value: String,
    onChange: (String) -> Unit,
) {
    BasicTextField(
        value = value,
        onValueChange = onChange,
        singleLine = true,
        textStyle = TextStyle(fontSize = TEXT_SP.sp, color = Color.White),
        modifier =
            Modifier
                .fillMaxWidth()
                .background(Color(FIELD), RoundedCornerShape(8.dp))
                .padding(horizontal = 10.dp, vertical = 8.dp),
    )
}

@Composable
private fun Title(text: String) {
    BasicText(text, style = TextStyle(fontSize = TITLE_SP.sp, color = Color(PANEL_TEXT)))
}

@Composable
private fun Label(text: String) {
    BasicText(text, style = TextStyle(fontSize = LABEL_SP.sp, color = Color(PANEL_MUTED)))
}
