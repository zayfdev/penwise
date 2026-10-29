//! `POST /frames/open` and `GET /frames/<id>/snapshot.png` (design-harness C2): an agent creates the frames it
//! proposes and shows the owner what it sees.
//!
//! Opening a frame puts it on a page — the caller's, else the project's `"<project> review"` page (created on
//! first use, in the project's notebook) — to the right of the frames already there, so the owner's tablet and
//! the PC window show it live. A snapshot is the frame's route at its preset through the shared headless browser,
//! with the owner's ink composited by core when `ink=true`.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::http::header;
use axum::response::{IntoResponse, Response};
use penwise_core::Bounds;
use penwise_core::item::{FramePreset, FrameSpec, ItemId, ItemKind, Layer, Z_STEP, add_frame};
use penwise_core::marks::MarkView;
use penwise_core::raster::png::encode_png;
use penwise_core::raster::render_scene_onto;
use penwise_proto::notebook::POSITION_STEP;
use penwise_proto::{Background, ItemRef, NotebookId, PageChange, PageId, PageKind};
use serde::Deserialize;
use serde_json::{Value, json};
use tiny_skia::Pixmap;

use crate::api::{ApiError, AppState, Author, page_of};
use crate::frames::registry::Project;
use crate::inspect::Clip;
use crate::storage::random::random_u128;

use super::frame::FrameView;
use super::scene::{frame_item, frame_keys, frame_position};

/// Gap between a new frame and the rightmost existing one, in page px.
const FRAME_GAP: f32 = 40.0;
/// Fallback page position step, matching the catalog.
const PAGE_POSITION_STEP: i64 = POSITION_STEP;

/// `POST /frames/open {"project","route","preset","page"?}`.
#[derive(Debug, Deserialize)]
pub(crate) struct OpenBody {
    project: String,
    route: String,
    preset: String,
    page: Option<String>,
}

/// `POST /frames/open` — `{"page": <id>, "frame": <id>}`.
pub(crate) async fn frame_open(
    State(app): State<AppState>,
    Json(body): Json<OpenBody>,
) -> Result<Json<Value>, ApiError> {
    let project = app
        .projects
        .get(&body.project)
        .await?
        .ok_or_else(|| ApiError::not_found("unknown project"))?;
    let preset = FramePreset::parse(&body.preset)
        .ok_or_else(|| ApiError::bad_request("preset is one of phone, tablet, desktop, custom"))?;
    let page = match &body.page {
        Some(id) => page_of(&app, Some(id)).await?,
        None => review_page(&app, &project).await?,
    };
    let view = app
        .storage
        .page_view(page)
        .await?
        .filter(|view| !view.page.deleted)
        .ok_or_else(|| ApiError::not_found("unknown page"))?;
    let scene = view.replica.scene();
    let x = next_x(scene);
    let spec = FrameSpec::new(project.id, body.route, preset, None, None, x, 0.0)
        .map_err(|error| ApiError::bad_request(error.to_string()))?;
    let z = scene.next_z().unwrap_or(Z_STEP);
    let id: ItemId = ItemRef::from_parts(app.storage.now_ms(), random_u128()).into();
    let mut author = Author::new(&app.storage);
    author
        .item(page, add_frame(&spec, id, Layer::Ink, z))
        .await?;
    author.commit().await?;
    Ok(Json(
        json!({ "page": page.to_string(), "frame": id.to_hex() }),
    ))
}

/// The project's review page, created in its notebook when it does not exist yet.
async fn review_page(app: &AppState, project: &Project) -> Result<PageId, ApiError> {
    let title = format!("{} review", project.name);
    let catalog = app.storage.catalog().await?;
    let known = project
        .notebook
        .as_deref()
        .and_then(|text| text.parse::<NotebookId>().ok())
        .filter(|id| {
            catalog
                .notebook(*id)
                .is_some_and(|notebook| !notebook.deleted)
        });
    if let Some(notebook) = known {
        let pages = catalog.pages(notebook);
        if let Some(page) = pages.iter().find(|page| page.title == title) {
            return Ok(page.id);
        }
    }
    let mut author = Author::new(&app.storage);
    let notebook = match known {
        Some(notebook) => notebook,
        None => crate::api::pages::pick_notebook(app, &catalog, None, &mut author).await?,
    };
    let pages = catalog.pages(notebook);
    if let Some(page) = pages.iter().find(|page| page.title == title) {
        return Ok(page.id);
    }
    let position = pages.last().map_or(PAGE_POSITION_STEP, |page| {
        page.position + PAGE_POSITION_STEP
    });
    let id = PageId::from_parts(app.storage.now_ms(), random_u128());
    author
        .page(
            id,
            PageChange::Create {
                notebook,
                title,
                position,
                kind: PageKind::Infinite,
                background: Background::Blank,
            },
        )
        .await?;
    author.commit().await?;
    Ok(id)
}

/// One gap to the right of the page's rightmost frame.
fn next_x(scene: &penwise_core::item::Scene) -> f32 {
    let mut right: Option<f32> = None;
    for id in scene.live_ids() {
        let Some(item) = scene.get(id) else {
            continue;
        };
        let ItemKind::Frame(frame) = &item.kind else {
            continue;
        };
        let edge = item.transform.e + frame.page_size()[0];
        right = Some(right.map_or(edge, |current| current.max(edge)));
    }
    right.map_or(0.0, |edge| edge + FRAME_GAP)
}

/// `GET /frames/<id>/snapshot.png?ink=true|false`.
#[derive(Debug, Deserialize)]
pub(crate) struct SnapshotQuery {
    ink: Option<String>,
}

/// The frame as PNG at its preset size, the owner's ink composited when `ink` is not `false`/`0`/`no`.
pub(crate) async fn frame_snapshot(
    State(app): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<SnapshotQuery>,
) -> Result<Response, ApiError> {
    let frame_id = ItemId::from_hex(&id)
        .ok_or_else(|| ApiError::bad_request("frame id is 32 hex characters"))?;
    let page = app
        .storage
        .page_of_item(frame_id)
        .await?
        .ok_or_else(|| ApiError::not_found("unknown frame"))?;
    let view = app
        .storage
        .page_view(page)
        .await?
        .filter(|view| !view.page.deleted)
        .ok_or_else(|| ApiError::not_found("unknown frame"))?;
    let scene = view.replica.scene();
    let frame = frame_item(scene, frame_id)
        .cloned()
        .ok_or_else(|| ApiError::not_found("unknown frame"))?;
    let position = frame_position(scene, frame_id);
    let keys = frame_keys(scene, frame_id);
    let browser = FrameView::open(
        app.inspect.as_ref(),
        &app.projects,
        frame_id,
        &frame,
        &keys,
        "",
    )
    .await
    .map_err(|error| ApiError::new(StatusCode::SERVICE_UNAVAILABLE, error.to_string()))?;
    let size = frame.page_size();
    let raw = app
        .inspect
        .snapshot(
            &browser.page,
            Some(Clip::new(0.0, 0.0, f64::from(size[0]), f64::from(size[1]))),
        )
        .await
        .map_err(|error| ApiError::new(StatusCode::SERVICE_UNAVAILABLE, error.to_string()))?;
    let png = if ink_enabled(query.ink.as_deref()) {
        composite(&raw, scene, &browser, position, size)?
    } else {
        raw
    };
    Ok((
        [
            (header::CONTENT_TYPE, "image/png"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        png,
    )
        .into_response())
}

fn ink_enabled(ink: Option<&str>) -> bool {
    !matches!(ink, Some("false" | "0" | "no"))
}

/// The snapshot with every visible mark of the frame composited where core places it.
fn composite(
    raw: &[u8],
    scene: &penwise_core::item::Scene,
    browser: &FrameView,
    position: [f32; 2],
    size: [f32; 2],
) -> Result<Vec<u8>, ApiError> {
    let presentation = MarkView::new(scene, &browser.layouts, false).presentation();
    let ink = crate::render::ink_only(&presentation.scene)
        .map_err(|error| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;
    let mut pixmap = Pixmap::decode_png(raw)
        .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "snapshot decode failed"))?;
    let region = Bounds {
        min_x: position[0],
        min_y: position[1],
        max_x: position[0] + size[0],
        max_y: position[1] + size[1],
    };
    render_scene_onto(
        &mut pixmap,
        &ink,
        penwise_core::item::LayerSet::INK,
        &region,
        1.0,
        &(),
    )
    .map_err(|error| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;
    encode_png(&pixmap)
        .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "png encode failed"))
}
