//! The history picture's ink: the frame's own snapshot (no ink) with only the round's strokes composited, where
//! core places them, in the snapshot's own pixel space (a snapshot too large for the canvas is scaled down, and
//! the ink is drawn at the same scale). Marks drawn after the round was sent belong to the next round and stay off it.

use std::collections::BTreeSet;

use axum::http::StatusCode;
use penwise_core::Bounds;
use penwise_core::item::{ItemId, ItemKind, LayerSet, Scene};
use penwise_core::marks::MarkView;
use penwise_core::raster::render_scene_onto;
use tiny_skia::Pixmap;

use crate::api::{ApiError, AppState};

use super::super::frame::FrameView;
use super::super::scene::{frame_item, frame_keys, frame_position};

/// `pixmap` (the whole box of `frame`, no ink, scaled by `scale` from `box_size` CSS px) with `strokes`
/// composited on it.
///
/// # Errors
/// 503 when the frame page cannot be opened, 500 on a raster failure.
pub(super) async fn composite_strokes(
    app: &AppState,
    scene: &Scene,
    frame: ItemId,
    strokes: &BTreeSet<ItemId>,
    snapshot: Snapshot,
) -> Result<Pixmap, ApiError> {
    let item = frame_item(scene, frame).ok_or_else(|| ApiError::not_found("unknown frame"))?;
    let keys = frame_keys(scene, frame);
    let browser = FrameView::open(app.inspect.as_ref(), &app.projects, frame, item, &keys, "")
        .await
        .map_err(|error| ApiError::new(StatusCode::SERVICE_UNAVAILABLE, error.to_string()))?;
    let presentation = MarkView::new(scene, &browser.layouts, true).presentation();
    let only = only_strokes(&presentation.scene, strokes)?;
    draw_strokes(snapshot, &only, frame_position(scene, frame))
}

/// A snapshot as the frame handler returns it: the pixels, the factor they were scaled by and the box in CSS px.
pub(super) struct Snapshot {
    pub(super) pixmap: Pixmap,
    /// Pixels per CSS px (`1` unless the canvas had to shrink).
    pub(super) scale: f32,
    /// The frame's box in CSS px: `css_w` by the document height.
    pub(super) box_size: [f32; 2],
}

/// Draws `ink` (page coordinates) onto the snapshot, whose top-left is the frame at `position`.
pub(super) fn draw_strokes(
    snapshot: Snapshot,
    ink: &Scene,
    position: [f32; 2],
) -> Result<Pixmap, ApiError> {
    let Snapshot {
        mut pixmap,
        scale,
        box_size,
    } = snapshot;
    let region = Bounds {
        min_x: position[0],
        min_y: position[1],
        max_x: position[0] + box_size[0],
        max_y: position[1] + box_size[1],
    };
    render_scene_onto(&mut pixmap, ink, LayerSet::INK, &region, scale, &())
        .map_err(|error| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;
    Ok(pixmap)
}

/// The presentation scene restricted to `strokes` and the masks that cut them.
fn only_strokes(ink: &Scene, strokes: &BTreeSet<ItemId>) -> Result<Scene, ApiError> {
    let picked = ink
        .live_ids()
        .into_iter()
        .filter_map(|id| ink.get(id))
        .filter(|item| match &item.kind {
            ItemKind::Stroke(_) => strokes.contains(&item.id),
            ItemKind::Erase(erase) => strokes.contains(&erase.target),
            _ => false,
        });
    Scene::from_items(ink.kind(), picked.map(|item| (item.clone(), false)))
        .map_err(|error| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))
}
