//! `PUT /projects/<id>/strip-pages/<page>` and `GET /projects/<id>/strip-pages`: which sketch pages hang off
//! which screen's strip in the tablet's overview.

use axum::Json;
use axum::extract::{Path, State};
use penwise_core::item::FramePreset;
use penwise_proto::PageId;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::api::{ApiError, AppState};

use crate::agent::board_page;

use super::require_project;

/// `PUT` body: the strip.
#[derive(Debug, Deserialize)]
pub(super) struct AttachBody {
    route: String,
    preset: String,
}

/// `PUT /projects/<id>/strip-pages/<page> {"route","preset"}` — attaches the page to that strip; attaching it
/// again is a no-op (to another strip, a move). `{"page","route","preset"}`. 404 for an unknown project or page,
/// 400 for an empty route, a preset that is not a frame preset, the board page itself or a page outside the
/// project's board notebook.
pub(super) async fn attach(
    State(app): State<AppState>,
    Path((project, page)): Path<(String, String)>,
    Json(body): Json<AttachBody>,
) -> Result<Json<Value>, ApiError> {
    require_project(&app, &project).await?;
    let page: PageId = page
        .parse()
        .map_err(|_| ApiError::bad_request("page is a page id"))?;
    if body.route.is_empty() {
        return Err(ApiError::bad_request("route is required"));
    }
    let preset = FramePreset::parse(&body.preset)
        .ok_or_else(|| ApiError::bad_request("preset is one of phone, tablet, desktop, custom"))?
        .as_str();
    let view = app
        .storage
        .page_view(page)
        .await?
        .filter(|view| !view.page.deleted)
        .ok_or_else(|| ApiError::not_found("unknown page"))?;
    check_sketch_page(&app, &project, page, view.page.notebook).await?;
    app.storage
        .attach_strip_page(project, body.route.clone(), preset.to_owned(), page)
        .await?;
    Ok(Json(
        json!({ "page": page.to_string(), "route": body.route, "preset": preset }),
    ))
}

/// A strip holds sketch pages of the project's board notebook, never the board itself.
async fn check_sketch_page(
    app: &AppState,
    project: &str,
    page: PageId,
    notebook: penwise_proto::NotebookId,
) -> Result<(), ApiError> {
    let project = app
        .projects
        .get(project)
        .await?
        .ok_or_else(|| ApiError::not_found("unknown project"))?;
    let board = board_page(app, &project)
        .await?
        .ok_or_else(|| ApiError::bad_request("the project has no board yet"))?;
    if board == page {
        return Err(ApiError::bad_request("the board page is not a strip page"));
    }
    let board_notebook = app
        .storage
        .page_view(board)
        .await?
        .map(|view| view.page.notebook);
    if board_notebook != Some(notebook) {
        return Err(ApiError::bad_request(
            "the page is not in the project's board notebook",
        ));
    }
    Ok(())
}

/// `GET /projects/<id>/strip-pages` — `[{"page","route","preset"}]` in attach order; a page deleted from the
/// library is left out.
pub(super) async fn list(
    State(app): State<AppState>,
    Path(project): Path<String>,
) -> Result<Json<Value>, ApiError> {
    require_project(&app, &project).await?;
    let rows = app.storage.strip_pages(project).await?;
    Ok(Json(Value::Array(
        rows.into_iter()
            .map(|row| json!({ "page": row.page.to_string(), "route": row.route, "preset": row.preset }))
            .collect(),
    )))
}
