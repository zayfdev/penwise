//! Page → PNG through the shared scene renderer, so agents see exactly what the tablet draws: paper, then every
//! layer's items in paint order (`penwise_core::raster::render_scene` over the stored page's replica scene).
//!
//! `GET /page.png?page=<id>&scale=<s>` and the ink crop that text recognition reads both go through here; the
//! rendering itself runs on the blocking pool.

mod paper;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use axum::Router;
use axum::extract::{Query, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use penwise_core::Bounds;
use penwise_core::item::{ItemId, ItemKind, Layer, LayerSet, Scene};
use penwise_core::raster::png::encode_png;
use penwise_core::raster::{RasterError, render_scene_onto, scene_pixmap};
use penwise_proto::PageKind;
use tiny_skia::Pixmap;

use crate::api::{ApiError, AppState, page_of};
use crate::assets::{self, Assets};
use penwise_core::marks::{FrameLayouts, MarkView};

use crate::storage::PageView;

/// Smallest infinite-page render, in page px.
const MIN_INFINITE_WIDTH: f32 = 1200.0;
const MIN_INFINITE_HEIGHT: f32 = 900.0;
/// Breathing room around an infinite page's content, in page px.
const INFINITE_PADDING: f32 = 200.0;
/// Largest side of any rendered PNG, in pixels.
const MAX_SIDE_PX: f32 = 8000.0;
/// Largest side of an ink crop handed to a vision model: bigger costs seconds per pixel, not clarity.
pub(crate) const OCR_MAX_SIDE: f32 = 1400.0;
const MIN_SCALE: f32 = 0.05;
const MAX_SCALE: f32 = 4.0;

/// Why a render produced no PNG.
#[derive(Debug, thiserror::Error)]
pub(crate) enum RenderError {
    #[error("raster: {0}")]
    Raster(#[from] RasterError),
    #[error("scene: {0}")]
    Scene(#[from] penwise_core::edit::EditError),
    #[error("png encode failed")]
    Encode,
}

/// A page render's pixels and the page-space window they show.
struct Frame {
    region: Bounds,
    scale: f32,
}

/// Renders one page at `scale` pixels per page px: white paper, the background pattern, then the scene.
pub(crate) fn page_png(
    view: &PageView,
    assets: &Assets,
    scale: f32,
) -> Result<Vec<u8>, RenderError> {
    let raw = view.replica.scene();
    // The server has no live layout: bound/area strokes render at their binding anchor (the page position they
    // were drawn at) and done marks stay hidden, exactly as `penwise_core::marks` specifies.
    let layouts = FrameLayouts::new();
    let marks = MarkView::new(raw, &layouts, false);
    let presentation = marks.has_marks().then(|| marks.presentation());
    let scene = presentation.as_ref().map_or(raw, |p| &p.scene);
    let frame = page_frame(view, scale);
    // The paper goes down first and the ink is drawn onto it, so highlighter (multiply) strokes darken the
    // paper like they do on the tablet and in the browser.
    let mut paper = scene_pixmap(&frame.region, frame.scale)?;
    paper.fill(tiny_skia::Color::WHITE);
    paper::draw_background(
        &mut paper,
        view.page.background,
        [frame.region.min_x, frame.region.min_y],
        frame.scale,
    );
    render_scene_onto(
        &mut paper,
        scene,
        LayerSet::ALL,
        &frame.region,
        frame.scale,
        &asset_pixmaps(scene, assets),
    )?;
    encode_png(&paper).map_err(|_| RenderError::Encode)
}

/// The handwriting of a page, ready for recognition.
#[derive(Debug)]
pub(crate) struct InkCrop {
    pub png: Vec<u8>,
    /// Page-space bounds of the strokes (without the padding).
    pub bounds: Bounds,
}

/// Just the handwriting: the ink-layer strokes' bounds, padded, at up to [`OCR_MAX_SIDE`] pixels per side.
///
/// Handwriting fills a corner of a page at most, and a full page render is both slower to run through a vision
/// model and worse to read. Pictures, shapes and agent marks are left out. `None` for a page with no ink.
pub(crate) fn ink_crop_png(
    view: &PageView,
    scale: f32,
    padding: f32,
) -> Result<Option<InkCrop>, RenderError> {
    let scene = strokes_only(view)?;
    let mut bounds = Bounds::EMPTY;
    for id in scene.render_order(LayerSet::INK) {
        if let Some(item) = scene.bounds(id) {
            bounds = bounds.union(&item);
        }
    }
    if bounds.is_empty() {
        return Ok(None);
    }
    let region = bounds.inflate(padding);
    let longest = region.width().max(region.height());
    let mut scale = sane_scale(scale);
    if longest * scale > OCR_MAX_SIDE {
        scale = (OCR_MAX_SIDE - 1.0) / longest;
    }
    let mut paper = scene_pixmap(&region, scale)?;
    paper.fill(tiny_skia::Color::WHITE);
    render_scene_onto(&mut paper, &scene, LayerSet::INK, &region, scale, &())?;
    let png = encode_png(&paper).map_err(|_| RenderError::Encode)?;
    Ok(Some(InkCrop { png, bounds }))
}

/// The window and scale of a page render: fixed pages start at 0,0; infinite ones wrap their content.
fn page_frame(view: &PageView, scale: f32) -> Frame {
    let scene = view.replica.scene();
    let region = match view.page.kind {
        PageKind::Fixed { width, height } => Bounds {
            min_x: 0.0,
            min_y: 0.0,
            max_x: width,
            max_y: height,
        },
        PageKind::Infinite => {
            let mut content = Bounds::EMPTY;
            for id in scene.render_order(LayerSet::ALL) {
                if let Some(item) = scene.bounds(id) {
                    content = content.union(&item);
                }
            }
            if content.is_empty() {
                content = Bounds {
                    min_x: 0.0,
                    min_y: 0.0,
                    max_x: 0.0,
                    max_y: 0.0,
                };
            }
            let min_x = content.min_x - INFINITE_PADDING;
            let min_y = content.min_y - INFINITE_PADDING;
            Bounds {
                min_x,
                min_y,
                max_x: min_x + (content.max_x - min_x + INFINITE_PADDING).max(MIN_INFINITE_WIDTH),
                max_y: min_y + (content.max_y - min_y + INFINITE_PADDING).max(MIN_INFINITE_HEIGHT),
            }
        }
    };
    let mut scale = sane_scale(scale);
    let longest = region.width().max(region.height());
    if longest * scale > MAX_SIDE_PX {
        scale = (MAX_SIDE_PX - 1.0) / longest;
    }
    Frame { region, scale }
}

fn sane_scale(scale: f32) -> f32 {
    if scale.is_finite() && scale > 0.0 {
        scale.clamp(MIN_SCALE, MAX_SCALE)
    } else {
        1.0
    }
}

/// The decoded pictures the scene's live image items refer to. A picture whose asset is missing or does not
/// decode is skipped with a warning: one broken asset must not fail a render an agent asked for.
fn asset_pixmaps(scene: &Scene, assets: &Assets) -> BTreeMap<[u8; 32], Pixmap> {
    let mut pictures = BTreeMap::new();
    for id in scene.render_order(LayerSet::ALL) {
        let Some(ItemKind::Image(image)) = scene.get(id).map(|item| &item.kind) else {
            continue;
        };
        if pictures.contains_key(&image.asset) {
            continue;
        }
        let name = assets::id_of(&image.asset);
        match assets.bytes(&name).map(|bytes| Pixmap::decode_png(&bytes)) {
            Some(Ok(pixmap)) => {
                pictures.insert(image.asset, pixmap);
            }
            Some(Err(_)) => tracing::warn!(asset = %name, "picture could not be decoded"),
            None => tracing::warn!(asset = %name, "picture asset missing"),
        }
    }
    pictures
}

/// Ids of the page's live ink-layer strokes: what handwriting recognition reads.
pub(crate) fn ink_stroke_ids(view: &PageView) -> BTreeSet<ItemId> {
    view.replica
        .items()
        .filter(|state| !state.deleted)
        .map(|state| &state.item)
        .filter(|item| item.layer == Layer::Ink && matches!(item.kind, ItemKind::Stroke(_)))
        .map(|item| item.id)
        .collect()
}

/// The page's live ink-layer strokes and their precise-erase masks, nothing else.
fn strokes_only(view: &PageView) -> Result<Scene, RenderError> {
    let kept = ink_stroke_ids(view);
    let picked = view
        .replica
        .items()
        .filter(|state| !state.deleted)
        .map(|state| &state.item)
        .filter(|item| match &item.kind {
            ItemKind::Erase(erase) => kept.contains(&erase.target),
            _ => kept.contains(&item.id),
        });
    Ok(Scene::from_items(
        view.replica.scene().kind(),
        picked.map(|item| (item.clone(), false)),
    )?)
}

#[derive(serde::Deserialize)]
struct PngQuery {
    page: Option<String>,
    scale: Option<f32>,
}

/// `GET /page.png?page=<id>&scale=1` — one page as PNG (`scale` is pixels per page px; `page` defaults to the
/// current page).
async fn page_png_route(
    State(app): State<AppState>,
    Query(query): Query<PngQuery>,
) -> Result<Response, ApiError> {
    let page = page_of(&app, query.page.as_deref()).await?;
    let view = app
        .storage
        .page_view(page)
        .await?
        .ok_or_else(|| ApiError::not_found("unknown page"))?;
    let assets = Arc::clone(&app.assets);
    let scale = query.scale.unwrap_or(1.0);
    let png = tokio::task::spawn_blocking(move || page_png(&view, &assets, scale))
        .await
        .map_err(|error| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?
        .map_err(|error| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;
    Ok((
        [
            (header::CONTENT_TYPE, "image/png"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        png,
    )
        .into_response())
}

/// `GET /page.png`.
pub(crate) fn routes() -> Router<AppState> {
    Router::new().route("/page.png", get(page_png_route))
}

#[cfg(test)]
mod tests;
