//! `show {sketch}` and `show {image}`: the agent draws and places pictures on the board.

use penwise_core::item::ItemId;
use penwise_proto::PageId;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::annotate::{AnnotateRequest, AnnotatedStroke};
use crate::api::marks::add_annotations;
use crate::api::{ApiError, AppState};
use crate::frames::registry::Project;
use crate::pictures::{PlaceRequest, place_picture};

use super::super::board::board_page;
use super::super::scene::frame_position;
use super::{frame_id, page_of_frame};

/// Gap between the board's content and a picture placed without a position, in page px.
const IMAGE_GAP: f32 = 40.0;

/// `sketch {strokes, near?}`: polylines on the agent layer; with `near` (a frame id) the points are relative to
/// that frame's top-left corner.
#[derive(Debug, Deserialize)]
pub(crate) struct SketchArgs {
    strokes: Vec<AnnotatedStroke>,
    #[serde(default)]
    near: Option<String>,
}

/// `image {asset, w, h, x?, y?, label?}`: a picture already uploaded to the asset store.
#[derive(Debug, Deserialize)]
pub(crate) struct ImageArgs {
    asset: String,
    w: f32,
    h: f32,
    #[serde(default)]
    x: Option<f32>,
    #[serde(default)]
    y: Option<f32>,
    #[serde(default)]
    label: Option<String>,
}

/// The page to draw on and the board origin of the drawing.
async fn target(
    app: &AppState,
    project: &Project,
    near: Option<&str>,
) -> Result<(PageId, Option<ItemId>, [f32; 2]), ApiError> {
    if let Some(text) = near {
        let frame = frame_id(text)?;
        let page = page_of_frame(app, frame).await?;
        let view = app
            .storage
            .page_view(page)
            .await?
            .ok_or_else(|| ApiError::not_found("unknown frame"))?;
        return Ok((
            page,
            Some(frame),
            frame_position(view.replica.scene(), frame),
        ));
    }
    let page = board_page(app, project)
        .await?
        .ok_or_else(|| ApiError::not_found("no board yet: show a frame first"))?;
    Ok((page, None, [0.0, 0.0]))
}

pub(super) async fn sketch(
    app: &AppState,
    project: &Project,
    args: SketchArgs,
) -> Result<Value, ApiError> {
    let (page, frame, origin) = target(app, project, args.near.as_deref()).await?;
    let mut strokes = args.strokes;
    for stroke in &mut strokes {
        for point in &mut stroke.points {
            point[0] += origin[0];
            point[1] += origin[1];
        }
    }
    let added = add_annotations(
        app,
        page,
        &AnnotateRequest {
            page: None,
            strokes,
        },
    )
    .await?;
    (app.announce_focus)(page, frame);
    Ok(json!({ "added": added, "page": page.to_string() }))
}

pub(super) async fn image(
    app: &AppState,
    project: &Project,
    args: ImageArgs,
) -> Result<Value, ApiError> {
    let (page, _, _) = target(app, project, None).await?;
    let (x, y) = match (args.x, args.y) {
        (Some(x), Some(y)) => (x, y),
        (x, y) => {
            let (free_x, free_y) = free_spot(app, page).await?;
            (x.unwrap_or(free_x), y.unwrap_or(free_y))
        }
    };
    let request: PlaceRequest = serde_json::from_value(json!({
        "asset": args.asset,
        "x": x,
        "y": y,
        "w": args.w,
        "h": args.h,
        "label": args.label,
        "layer": "agent",
    }))
    .map_err(|error| ApiError::bad_request(error.to_string()))?;
    let picture = place_picture(app, &page.to_string(), request).await?;
    (app.announce_focus)(page, None);
    serde_json::to_value(picture).map_err(|error| ApiError::bad_request(error.to_string()))
}

/// Right of everything on the page, at the height of its top.
async fn free_spot(app: &AppState, page: PageId) -> Result<(f32, f32), ApiError> {
    let view = app
        .storage
        .page_view(page)
        .await?
        .ok_or_else(|| ApiError::not_found("unknown page"))?;
    let scene = view.replica.scene();
    let mut bounds = penwise_core::Bounds::EMPTY;
    for id in scene.live_ids() {
        if let Some(item) = scene.bounds(id) {
            bounds = bounds.union(&item);
        }
    }
    if bounds.is_empty() {
        return Ok((0.0, 0.0));
    }
    Ok((bounds.max_x + IMAGE_GAP, bounds.min_y))
}
