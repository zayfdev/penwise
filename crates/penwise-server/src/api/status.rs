//! `GET /health`: counts for scripts and agents (`penwise_page_status`).

use axum::Json;
use axum::Router;
use axum::extract::{Query, State};
use axum::routing::get;
use penwise_core::item::ItemKind;
use serde::Deserialize;
use serde_json::{Value, json};

use super::AppState;
use super::error::ApiError;
use super::resolve::page_of;

pub(super) fn routes() -> Router<AppState> {
    Router::new().route("/health", get(health))
}

#[derive(Debug, Deserialize)]
struct PageQuery {
    page: Option<String>,
}

async fn health(
    State(app): State<AppState>,
    Query(query): Query<PageQuery>,
) -> Result<Json<Value>, ApiError> {
    let status = app.storage.status().await?;
    let catalog = app.storage.catalog().await?;
    let pages: usize = catalog
        .notebooks()
        .iter()
        .map(|nb| catalog.pages(nb.id).len())
        .sum();
    if pages == 0 && query.page.is_none() {
        return Ok(Json(json!({
            "ok": true, "page": null, "strokes": 0, "pages": 0, "head_seq": status.head_seq,
        })));
    }
    let page = page_of(&app, query.page.as_deref()).await?;
    let view = app
        .storage
        .page_view(page)
        .await?
        .ok_or_else(|| ApiError::not_found("unknown page"))?;
    let strokes = view
        .replica
        .items()
        .filter(|m| !m.deleted && matches!(m.item.kind, ItemKind::Stroke(_)))
        .count();
    Ok(Json(json!({
        "ok": true,
        "page": page.to_string(),
        "strokes": strokes,
        "pages": pages,
        "head_seq": status.head_seq,
    })))
}
