//! Handwriting → text through a local vision model (Ollama); the result is stored as text-layer items.
//!
//! Nothing leaves the machine: the cropped ink is posted to the configured model endpoint
//! (`AppState::ocr_url`, default a local Ollama). The recognition becomes an `Add(Text)` item on `Layer::Text`
//! whose `sources` are the strokes it was read from, authored by the server device, so it syncs to every device
//! and `/search` finds it. The cache is the item itself: it is fresh exactly while the live ink strokes of the
//! page equal the union of the sources of its text-layer items, so any new, erased or moved-away stroke makes
//! the next `/text` read the page again. When no model is reachable the page simply has no text — the ink and
//! the rest of the surface keep working.

mod ollama;

use std::collections::BTreeSet;
use std::sync::Arc;

use axum::Router;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use penwise_core::brush::Color;
use penwise_core::edit::Edit;
use penwise_core::item::{Affine, Item, ItemId, ItemKind, Layer, Paint, TextItem, Z_STEP};
use penwise_proto::{ItemRef, PageId};

use crate::api::{ApiError, AppState, Author, page_of};
use crate::render::{self, ink_stroke_ids};
use crate::storage::PageView;
use crate::textlayer::{self, TextRow};

/// Margin around the ink when it is cropped for recognition, in page px.
const OCR_PADDING_PX: f32 = 48.0;
const MIN_SCALE: f32 = 0.5;
const MAX_SCALE: f32 = 2.0;
/// Font size recorded on a recognized block, local px (text is not drawn by the server; hosts lay it out).
const RECOGNIZED_FONT_PX: f32 = 24.0;

/// One recognition at a time: the model is CPU-bound, and two concurrent reads of one page would add its text
/// twice.
static RECOGNITION: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// A page's text and whether it came from the stored recognition without calling the model.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct PageText {
    pub text: String,
    pub cached: bool,
}

/// Renders `scale` to use: pages are read at a scale that keeps small handwriting legible without huge images.
#[must_use]
pub(crate) fn read_scale(requested: Option<f32>) -> f32 {
    requested
        .filter(|scale| scale.is_finite())
        .map_or(1.0, |scale| scale.clamp(MIN_SCALE, MAX_SCALE))
}

/// Strokes the stored recognition covers.
fn covered(rows: &[TextRow]) -> BTreeSet<ItemId> {
    rows.iter()
        .filter(|row| row.layer == Layer::Text)
        .flat_map(|row| row.sources.iter().copied())
        .collect()
}

fn recognized_item(
    id: ItemId,
    view: &PageView,
    bounds: &penwise_core::Bounds,
    text: String,
    sources: &BTreeSet<ItemId>,
) -> Item {
    Item {
        id,
        layer: Layer::Text,
        z: view.replica.scene().next_z().unwrap_or(Z_STEP),
        transform: Affine::translate(bounds.min_x, bounds.min_y),
        kind: ItemKind::Text(TextItem {
            content: text,
            size: [bounds.width().max(1.0), bounds.height().max(1.0)],
            font_size: RECOGNIZED_FONT_PX,
            paint: Paint {
                color: Color::INK,
                opacity: 1.0,
            },
            sources: sources.iter().copied().collect(),
        }),
    }
}

async fn load(app: &AppState, page: PageId) -> Result<PageView, ApiError> {
    app.storage
        .page_view(page)
        .await?
        .ok_or_else(|| ApiError::not_found("unknown page"))
}

/// The page's text, reading the handwriting through the model when the stored recognition is missing or stale
/// (or `refresh`).
///
/// # Errors
/// 404 for an unknown page, 503 when the model cannot be reached, 500 for render failures.
pub(crate) async fn page_text(
    app: &AppState,
    page: PageId,
    refresh: bool,
    scale: f32,
) -> Result<PageText, ApiError> {
    let _one_at_a_time = RECOGNITION.lock().await;
    let view = load(app, page).await?;
    let rows = textlayer::rows_of(&view);
    let ink = ink_stroke_ids(&view);
    let recognized = rows.iter().filter(|row| row.layer == Layer::Text).count();
    if !refresh && covered(&rows) == ink {
        return Ok(PageText {
            text: textlayer::joined(&rows),
            cached: recognized > 0,
        });
    }
    let view = Arc::new(view);
    let crop = {
        let view = Arc::clone(&view);
        tokio::task::spawn_blocking(move || render::ink_crop_png(&view, scale, OCR_PADDING_PX))
            .await
            .map_err(|error| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?
            .map_err(|error| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?
    };
    let mut author = Author::new(&app.storage);
    for row in rows.iter().filter(|row| row.layer == Layer::Text) {
        author.item(page, Edit::Delete(row.id)).await?;
    }
    if let Some(crop) = crop {
        let text = ollama::transcribe(&app.ocr_url, crop.png)
            .await
            .map_err(|error| {
                ApiError::new(
                    StatusCode::SERVICE_UNAVAILABLE,
                    format!("handwriting model unavailable: {error}"),
                )
            })?;
        let id =
            ItemRef::from_parts(app.storage.now_ms(), crate::storage::random::random_u128()).into();
        let item = recognized_item(id, &view, &crop.bounds, text, &ink);
        author.item(page, Edit::Add(item)).await?;
    }
    author.commit().await?;
    let rows = textlayer::rows_of(&load(app, page).await?);
    Ok(PageText {
        text: textlayer::joined(&rows),
        cached: false,
    })
}

/// Query of the text route.
#[derive(serde::Deserialize)]
struct TextQuery {
    page: Option<String>,
    refresh: Option<bool>,
    scale: Option<f32>,
}

/// `GET /text?page=<id>&refresh=1` — the page's handwriting as text.
async fn text_route(
    State(app): State<AppState>,
    Query(query): Query<TextQuery>,
) -> Result<Response, ApiError> {
    let page = page_of(&app, query.page.as_deref()).await?;
    let found = page_text(
        &app,
        page,
        query.refresh.unwrap_or(false),
        read_scale(query.scale),
    )
    .await?;
    Ok(axum::Json(serde_json::json!({
        "page": page.to_string(),
        "text": found.text,
        "cached": found.cached,
    }))
    .into_response())
}

/// `GET /text`.
pub(crate) fn routes() -> Router<AppState> {
    Router::new().route("/text", get(text_route))
}

#[cfg(test)]
mod tests;
