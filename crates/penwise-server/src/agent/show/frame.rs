//! `show {frame}`: open a route of the project on its board, or reuse the frame already showing it.

use penwise_core::item::{FrameExtent, FramePreset, ItemId, set_extent};
use penwise_proto::PageId;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::api::{ApiError, AppState, Author};
use crate::frames::registry::Project;

use super::super::board::{board_page, find_frame};
use super::super::frame_open_via_json;
use super::super::scene::frame_item;

/// `frame {route, preset?="desktop", extent?="full"}`.
#[derive(Debug, Deserialize)]
pub(crate) struct FrameArgs {
    route: String,
    #[serde(default)]
    preset: Option<String>,
    #[serde(default)]
    extent: Option<String>,
}

/// Opens (or reuses) the frame and points the owner's camera at it.
pub(super) async fn open(
    app: &AppState,
    project: &Project,
    args: FrameArgs,
) -> Result<Value, ApiError> {
    let preset_name = args.preset.as_deref().unwrap_or("desktop");
    let preset = FramePreset::parse(preset_name)
        .ok_or_else(|| ApiError::bad_request("preset is one of phone, tablet, desktop, custom"))?;
    let extent = match args.extent.as_deref() {
        None => FrameExtent::Full,
        Some(text) => FrameExtent::parse(text)
            .ok_or_else(|| ApiError::bad_request("extent is full or viewport"))?,
    };
    let (page, frame, reused) =
        if let Some((page, frame)) = existing(app, project, &args.route, preset).await? {
            (page, frame, true)
        } else {
            let (page, frame) = frame_open_via_json(app, &project.id, &args.route, preset).await?;
            (page, frame, false)
        };
    apply_extent(app, page, frame, extent).await?;
    // The next lane is placed under this frame's box, so its document height must be known first.
    wait_for_height(app, page, frame).await?;
    (app.announce_focus)(page, Some(frame));
    Ok(json!({
        "page": page.to_string(),
        "frame": frame.to_hex(),
        "route": args.route,
        "preset": preset.as_str(),
        "extent": extent.as_str(),
        "reused": reused,
    }))
}

/// How long `show frame` waits for the frame's first still.
const FIRST_STILL_WAIT: std::time::Duration = std::time::Duration::from_secs(20);

/// A frame with no measured document height yet waits for its first still (which writes `FrameDocHeight`). On
/// timeout or failure the call goes on: the capture finishes in the background and re-stacks the lanes then.
async fn wait_for_height(app: &AppState, page: PageId, frame: ItemId) -> Result<(), ApiError> {
    let view = app
        .storage
        .page_view(page)
        .await?
        .ok_or_else(|| ApiError::not_found("unknown page"))?;
    if frame_item(view.replica.scene(), frame).is_some_and(|item| item.doc_h == 0) {
        app.stills.first_still(frame, FIRST_STILL_WAIT).await;
    }
    Ok(())
}

/// The frame of the board already showing `route` at `preset`.
async fn existing(
    app: &AppState,
    project: &Project,
    route: &str,
    preset: FramePreset,
) -> Result<Option<(PageId, ItemId)>, ApiError> {
    let Some(page) = board_page(app, project).await? else {
        return Ok(None);
    };
    let Some(view) = app.storage.page_view(page).await? else {
        return Ok(None);
    };
    Ok(find_frame(view.replica.scene(), &project.id, route, preset).map(|frame| (page, frame)))
}

/// Gives the frame the asked extent (a new frame is `full` already).
async fn apply_extent(
    app: &AppState,
    page: PageId,
    frame: ItemId,
    extent: FrameExtent,
) -> Result<(), ApiError> {
    let view = app
        .storage
        .page_view(page)
        .await?
        .ok_or_else(|| ApiError::not_found("unknown page"))?;
    let scene = view.replica.scene();
    if frame_item(scene, frame).is_none_or(|item| item.extent == extent) {
        return Ok(());
    }
    let mut author = Author::new(&app.storage);
    for edit in set_extent(scene, frame, extent) {
        author.item(page, edit).await?;
    }
    author.commit().await?;
    Ok(())
}
