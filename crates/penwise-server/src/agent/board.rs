//! A project's board (spec v2 §2): its `"<name> review"` page, and the frames on it.

use penwise_core::item::{FrameItem, FramePreset, ItemId, ItemKind, Scene};
use penwise_proto::PageId;

use crate::api::{ApiError, AppState};
use crate::frames::registry::Project;

/// The project's board page, `None` until a frame has been opened for it.
///
/// # Errors
/// 500 when the catalog cannot be read.
pub(crate) async fn board_page(
    app: &AppState,
    project: &Project,
) -> Result<Option<PageId>, ApiError> {
    let title = format!("{} review", project.name);
    let catalog = app.storage.catalog().await?;
    let own = project
        .notebook
        .as_deref()
        .and_then(|text| text.parse::<penwise_proto::NotebookId>().ok());
    let mut notebooks: Vec<_> = catalog.notebooks().iter().map(|nb| nb.id).collect();
    notebooks.sort_by_key(|id| Some(*id) != own);
    for notebook in notebooks {
        if let Some(page) = catalog
            .pages(notebook)
            .iter()
            .find(|page| !page.deleted && page.title == title)
        {
            return Ok(Some(page.id));
        }
    }
    Ok(None)
}

/// The plain (non-variant) frame showing `route` of `project` at `preset`; the rightmost when several do (the
/// live one, after rounds moved it right).
pub(crate) fn find_frame(
    scene: &Scene,
    project: &str,
    route: &str,
    preset: FramePreset,
) -> Option<ItemId> {
    scene
        .live_ids()
        .into_iter()
        .filter_map(|id| {
            let item = scene.get(id)?;
            match &item.kind {
                ItemKind::Frame(frame) if is_plain(frame, project, route, preset) => {
                    Some((item.transform.e, id))
                }
                _ => None,
            }
        })
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, id)| id)
}

fn is_plain(frame: &FrameItem, project: &str, route: &str, preset: FramePreset) -> bool {
    frame.group.is_none()
        && frame.project == project
        && frame.route == route
        && frame.preset == preset
}
