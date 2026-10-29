//! Design-harness marks over the core (spec §6.3, K1/K2): `GET /pages/<id>/marks` and
//! `POST /marks/<id>/status`.
//!
//! Marks are ordinary items: grouped bound/area strokes plus an LWW status. The server has no bridge layout
//! (layouts are client-local), so the list renders rects through the core's anchor fallback and keeps done marks
//! hidden, exactly like `/page.png`. Status changes are server-device edits, so they sync, broadcast and undo
//! like every other op. Both routes sit in the `/projects` group: token, or a same-machine loopback request.

use axum::Json;
use axum::Router;
use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use penwise_core::item::{ItemId, ItemKind, Scene};
use penwise_core::marks::{
    FrameLayouts, MarkEntry, MarkStatus, MarkView, mark_of, set_mark_status,
};
use serde::Deserialize;
use serde_json::{Value, json};

use super::AppState;
use super::author::Author;
use super::error::ApiError;
use super::resolve::page_of;

pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/pages/{id}/marks", get(list))
        .route("/marks/{id}/status", post(status))
}

#[derive(Debug, Default, Deserialize)]
struct ListQuery {
    status: Option<String>,
}

/// `"?status=new|seen|done|reopened|all"`; `all` (the default) lists every mark.
fn status_filter(text: Option<&str>) -> Result<Option<MarkStatus>, ApiError> {
    match text.unwrap_or("all") {
        "all" => Ok(None),
        other => MarkStatus::parse(other).map(Some).ok_or_else(|| {
            ApiError::bad_request("status is one of new, seen, done, reopened, all")
        }),
    }
}

/// A mark with the frame facts an agent needs to locate it: `{"marks":[{"id", "frame", "project", "route",
/// "preset", "status", "state", "strokes", "rect"}]}`. `rect` is the effective page box, or `null` when the mark
/// is hidden (different preset, element gone, done without show-done).
async fn list(
    State(app): State<AppState>,
    Path(page_id): Path<String>,
    Query(query): Query<ListQuery>,
) -> Result<Json<Value>, ApiError> {
    let filter = status_filter(query.status.as_deref())?;
    let page = page_of(&app, Some(&page_id)).await?;
    let view = app
        .storage
        .page_view(page)
        .await?
        .filter(|view| !view.page.deleted)
        .ok_or_else(|| ApiError::not_found("unknown page"))?;
    let scene = view.replica.scene();
    let layouts = FrameLayouts::new();
    let entries = MarkView::new(scene, &layouts, false).mark_entries();
    let marks: Vec<Value> = entries
        .iter()
        .filter(|entry| filter.is_none_or(|status| entry.status == status))
        .map(|entry| entry_json(scene, entry))
        .collect();
    Ok(Json(json!({ "marks": marks })))
}

fn entry_json(scene: &Scene, entry: &MarkEntry) -> Value {
    let frame = match scene.get(entry.frame).map(|item| &item.kind) {
        Some(ItemKind::Frame(frame)) => Some(frame),
        _ => None,
    };
    json!({
        "id": entry.id.to_hex(),
        "frame": entry.frame.to_hex(),
        "project": frame.map(|frame| frame.project.clone()),
        "route": frame.map(|frame| frame.route.clone()),
        "preset": entry.preset.as_str(),
        "status": entry.status.as_str(),
        "state": entry.state,
        "strokes": entry.strokes.iter().map(|id| id.to_hex()).collect::<Vec<_>>(),
        "rect": entry.rect.map(|rect| json!([rect[0], rect[1], rect[2], rect[3]])),
    })
}

#[derive(Debug, Deserialize)]
struct StatusBody {
    status: String,
}

/// `POST /marks/<id>/status {"status"}` — one LWW status edit from the server device. Idempotent; a mark that
/// is already in that status gets no op.
async fn status(
    State(app): State<AppState>,
    Path(mark_id): Path<String>,
    Json(body): Json<StatusBody>,
) -> Result<Json<Value>, ApiError> {
    let status = MarkStatus::parse(&body.status)
        .ok_or_else(|| ApiError::bad_request("status is one of new, seen, done, reopened"))?;
    let id = ItemId::from_hex(&mark_id)
        .ok_or_else(|| ApiError::bad_request("mark id is 32 hex characters"))?;
    let page = app
        .storage
        .page_of_item(id)
        .await?
        .ok_or_else(|| ApiError::not_found("unknown mark"))?;
    let view = app
        .storage
        .page_view(page)
        .await?
        .ok_or_else(|| ApiError::not_found("unknown mark"))?;
    if mark_of(view.replica.scene(), id).is_none() {
        return Err(ApiError::not_found("unknown mark"));
    }
    let edits = set_mark_status(view.replica.scene(), id, status)
        .map_err(|error| ApiError::bad_request(error.to_string()))?;
    let mut author = Author::new(&app.storage);
    for edit in edits {
        author.item(page, edit).await?;
    }
    author.commit().await?;
    Ok(Json(json!({ "id": mark_id, "status": status.as_str() })))
}

#[cfg(test)]
mod tests;
