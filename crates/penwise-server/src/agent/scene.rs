//! Scene accessors the agent routes share (a frame item, its position, its keys, a mark's binding and strokes),
//! and rebuilding one mark's crops when an image URL misses the cache.

use penwise_core::item::{FrameItem, ItemId, ItemKind, Scene, StrokeItem};
use penwise_core::marks::{Binding, FrameLayouts, MarkEntry, MarkView, mark_of};

use crate::api::{ApiError, AppState};
use crate::storage::PageView;

use super::frame::FrameView;
use super::images::{self, MarkCrop};

/// Rebuilds and caches one mark's crops (an image request that misses the cache).
///
/// # Errors
/// 404 for an unknown/hidden mark, 503 when the frame page cannot be opened, 500 for a raster failure.
pub(crate) async fn images_for(
    app: &AppState,
    view: &PageView,
    mark: ItemId,
) -> Result<(), ApiError> {
    let scene = view.replica.scene();
    let Some(item) = mark_of(scene, mark) else {
        return Err(ApiError::not_found("unknown mark"));
    };
    let (frame_id, state, strokes) = (item.frame, item.state.clone(), item.strokes.clone());
    let Some(frame) = frame_item(scene, frame_id).cloned() else {
        return Err(ApiError::not_found("unknown frame"));
    };
    let position = frame_position(scene, frame_id);
    let keys = frame_keys(scene, frame_id);
    let browser = FrameView::open(
        app.inspect.as_ref(),
        &app.projects,
        frame_id,
        &frame,
        &keys,
        &state,
    )
    .await
    .map_err(|error| {
        ApiError::new(
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            error.to_string(),
        )
    })?;
    let marks = MarkView::new(scene, &browser.layouts, true);
    let presentation = marks.presentation();
    let Some(rect) = marks
        .mark_entries()
        .into_iter()
        .find(|entry| entry.id == mark)
        .and_then(|entry| entry.rect)
    else {
        return Err(ApiError::not_found("that mark is not visible"));
    };
    let ink = crate::render::ink_only(&presentation.scene).map_err(|error| {
        ApiError::new(
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            error.to_string(),
        )
    })?;
    let size = frame.page_size();
    let frame_box = [position[0], position[1], size[0], size[1]];
    let crops = [MarkCrop {
        mark,
        rect,
        strokes,
    }];
    let built = images::build(app.inspect.as_ref(), &browser, &ink, frame_box, &crops)
        .await
        .map_err(|error| {
            ApiError::new(
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                error.to_string(),
            )
        })?;
    match built.get(&mark) {
        Some(images) => {
            app.images.put(mark, view.updated_seq, images);
            Ok(())
        }
        None => Err(ApiError::not_found("no crop for that mark")),
    }
}

/// The frame item of a live frame.
pub(crate) fn frame_item(scene: &Scene, id: ItemId) -> Option<&FrameItem> {
    match &scene.get(id)?.kind {
        ItemKind::Frame(frame) => Some(frame),
        _ => None,
    }
}

/// A frame item's page position.
pub(crate) fn frame_position(scene: &Scene, id: ItemId) -> [f32; 2] {
    scene
        .get(id)
        .map_or([0.0, 0.0], |item| [item.transform.e, item.transform.f])
}

/// Every element key the frame's marks are bound to.
pub(crate) fn frame_keys(scene: &Scene, frame: ItemId) -> Vec<String> {
    let layouts = FrameLayouts::new();
    MarkView::new(scene, &layouts, true).keys(frame)
}

/// The binding of the mark's first bound stroke.
pub(crate) fn mark_binding<'a>(scene: &'a Scene, entry: &MarkEntry) -> Option<&'a Binding> {
    entry
        .strokes
        .iter()
        .find_map(|id| match &scene.get(*id)?.kind {
            ItemKind::Stroke(stroke) => stroke.binding.as_ref(),
            _ => None,
        })
}

/// The mark's member strokes, in group order.
pub(crate) fn mark_strokes<'a>(scene: &'a Scene, entry: &MarkEntry) -> Vec<&'a StrokeItem> {
    entry
        .strokes
        .iter()
        .filter_map(|id| match &scene.get(*id)?.kind {
            ItemKind::Stroke(stroke) => Some(stroke),
            _ => None,
        })
        .collect()
}
