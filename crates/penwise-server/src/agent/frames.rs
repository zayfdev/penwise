//! `POST /frames/open` and `GET /frames/<id>/snapshot.png` (design-harness C2): an agent creates the frames it
//! proposes and shows the owner what it sees.
//!
//! Opening a frame puts it on a page — the caller's, else the project's `"<project> board"` page (created on
//! first use, in the project's notebook), so the owner's tablet and the PC window show it live. The preset defaults to `desktop` and the frame to full length. Each screen (route
//! and preset) has a lane, and a new screen starts a new lane below the board ([`placement`]). The snapshot
//! ([`snapshot`]) is the frame's whole box through the shared headless browser, with the owner's ink composited.

use axum::Json;
use axum::extract::State;
use penwise_core::item::{
    FrameExtent, FramePreset, FrameSpec, ItemId, ItemKind, Layer, Z_STEP, add_frame,
};
use penwise_proto::notebook::POSITION_STEP;
use penwise_proto::{Background, ItemRef, NotebookId, PageChange, PageId, PageKind};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::api::{ApiError, AppState, Author, page_of};
use crate::frames::registry::Project;
use crate::storage::random::random_u128;

mod snapshot;

pub(crate) use snapshot::frame_snapshot;

/// Gap between a new frame and the rightmost existing one, in page px.
const FRAME_GAP: f32 = 40.0;
/// Gap between a lane and the lowest box above it.
const ROW_GAP: f32 = crate::frames::still::LANE_GAP;
/// Fallback page position step, matching the catalog.
const PAGE_POSITION_STEP: i64 = POSITION_STEP;

/// `POST /frames/open {"project","route","preset"?="desktop","extent"?="full","page"?}`.
#[derive(Debug, Deserialize)]
pub(crate) struct OpenBody {
    project: String,
    route: String,
    #[serde(default)]
    preset: Option<String>,
    #[serde(default)]
    extent: Option<String>,
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
    let preset = FramePreset::parse(body.preset.as_deref().unwrap_or("desktop"))
        .ok_or_else(|| ApiError::bad_request("preset is one of phone, tablet, desktop, custom"))?;
    let extent = match body.extent.as_deref() {
        None => FrameExtent::Full,
        Some(text) => FrameExtent::parse(text)
            .ok_or_else(|| ApiError::bad_request("extent is full or viewport"))?,
    };
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
    let [x, y] = placement(scene, &project.id, &body.route, preset);
    let spec = FrameSpec::new(project.id, body.route, preset, None, None, x, y)
        .map_err(|error| ApiError::bad_request(error.to_string()))?
        .with_extent(extent);
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

/// The project's board page (an older `"<project> review"` one counts), created in its notebook when it does not exist yet.
async fn review_page(app: &AppState, project: &Project) -> Result<PageId, ApiError> {
    let title = super::board::board_title(project);
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
        if let Some(page) = pages
            .iter()
            .find(|page| super::board::is_board_title(project, &page.title))
        {
            return Ok(page.id);
        }
    }
    let mut author = Author::new(&app.storage);
    let notebook = match known {
        Some(notebook) => notebook,
        None => crate::api::pages::pick_notebook(app, &catalog, None, &mut author).await?,
    };
    let pages = catalog.pages(notebook);
    if let Some(page) = pages
        .iter()
        .find(|page| super::board::is_board_title(project, &page.title))
    {
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

/// Where a new frame goes (spec v2 §2.1). Each screen — project, route and preset — is a **lane**: a row holding
/// that screen's frames. A screen that has a lane continues it: right of the lane's rightmost frame, on its row.
/// A new screen starts a lane below everything on the board, `ROW_GAP` under the lowest box, at the left edge of
/// the board; it never goes to the right of another lane's frame.
fn placement(
    scene: &penwise_core::item::Scene,
    project: &str,
    route: &str,
    preset: FramePreset,
) -> [f32; 2] {
    let mut board: Option<[f32; 3]> = None; // left, bottom, (unused)
    let mut lane: Option<[f32; 2]> = None; // right edge, row y
    for id in scene.live_ids() {
        let Some(item) = scene.get(id) else {
            continue;
        };
        let ItemKind::Frame(frame) = &item.kind else {
            continue;
        };
        // The box height is the document's (the synced hint), which the next lane stacks below.
        let [width, height] = frame.box_size(None);
        let (x, y) = (item.transform.e, item.transform.f);
        board = Some(board.map_or([x, y + height, 0.0], |[left, bottom, _]| {
            [left.min(x), bottom.max(y + height), 0.0]
        }));
        if frame.project == project && frame.route == route && frame.preset == preset {
            lane = Some(lane.map_or([x + width, y], |[right, row]| {
                [right.max(x + width), row.min(y)]
            }));
        }
    }
    match (lane, board) {
        (Some([right, row]), _) => [right + FRAME_GAP, row],
        (None, Some([left, bottom, _])) => [left, bottom + ROW_GAP],
        (None, None) => [0.0, 0.0],
    }
}
