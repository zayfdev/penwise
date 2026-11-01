//! Taking a frame's history picture: its snapshot without ink, the round's strokes on it, stored as an asset.

use std::collections::BTreeSet;

use axum::extract::{Path, Query, State};
use axum::http::{StatusCode, Uri};
use penwise_core::item::{ItemId, Scene};
use penwise_proto::ItemRef;
use tiny_skia::Pixmap;

use crate::api::{ApiError, AppState};
use crate::storage::random::random_u128;

use super::super::shrink::shrink;
use super::{MAX_SNAPSHOT_BYTES, Picture, STORED_SIDE, ink};

/// The picture of `frame` with `strokes` as ink, stored as an asset, to be placed at the frame's position.
pub(super) async fn picture_of(
    app: &AppState,
    scene: &Scene,
    frame: ItemId,
    label: String,
    strokes: &BTreeSet<ItemId>,
) -> Result<Picture, ApiError> {
    let position = match scene.get(frame) {
        Some(item) => [item.transform.e, item.transform.f],
        None => return Err(ApiError::not_found("unknown frame")),
    };
    let (png, scale, doc_height) = snapshot_png(app, frame).await?;
    let bare = Pixmap::decode_png(&png)
        .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "snapshot decode failed"))?;
    // The picture is the frame's whole box in page units, whatever the pixel scale of the snapshot.
    let box_size = [css_w(scene, frame)?, doc_height];
    let snapshot = ink::Snapshot {
        pixmap: bare,
        scale,
        box_size,
    };
    let inked = ink::composite_strokes(app, scene, frame, strokes, snapshot).await?;
    let asset = store_snapshot(app, inked).await?;
    let size = box_size;
    let place = super::super::scene::frame_item(scene, frame)
        .map(|item| (item.preset.as_str().to_owned(), item.route.clone()))
        .ok_or_else(|| ApiError::not_found("unknown frame"))?;
    Ok(Picture {
        frame,
        live: frame,
        slot: 0,
        variant: None,
        preset: place.0,
        route: place.1,
        id: ItemRef::from_parts(app.storage.now_ms(), random_u128()).into(),
        asset,
        position,
        size,
        label,
    })
}

/// The frame's snapshot without ink, through the existing `GET /frames/<id>/snapshot.png` handler: the PNG, the
/// scale it was shrunk by (`X-Penwise-Scale`) and the box height in CSS px (`X-Penwise-Doc-Height`).
async fn snapshot_png(app: &AppState, frame: ItemId) -> Result<(Vec<u8>, f32, f32), ApiError> {
    let query = Query::try_from_uri(&Uri::from_static("/?ink=false"))
        .map_err(|error| ApiError::bad_request(error.to_string()))?;
    let response =
        super::super::frames::frame_snapshot(State(app.clone()), Path(frame.to_hex()), query)
            .await?;
    let (scale, height) = {
        let headers = response.headers();
        (
            header_number(headers, "x-penwise-scale")?,
            header_number(headers, "x-penwise-doc-height")?,
        )
    };
    let bytes = axum::body::to_bytes(response.into_body(), MAX_SNAPSHOT_BYTES)
        .await
        .map_err(|error| ApiError::new(StatusCode::BAD_GATEWAY, error.to_string()))?;
    Ok((bytes.to_vec(), scale, height))
}

fn header_number(headers: &axum::http::HeaderMap, name: &str) -> Result<f32, ApiError> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .and_then(|text| text.parse::<f32>().ok())
        .filter(|value| value.is_finite() && *value > 0.0)
        .ok_or_else(|| ApiError::new(StatusCode::BAD_GATEWAY, format!("snapshot has no {name}")))
}

/// The frame's width in CSS px.
fn css_w(scene: &Scene, frame: ItemId) -> Result<f32, ApiError> {
    super::super::scene::frame_item(scene, frame)
        .map(|item| item.page_size()[0])
        .ok_or_else(|| ApiError::not_found("unknown frame"))
}

/// Stores `pixmap` as an asset (scaled down further when it is taller than the store accepts) and returns its id.
/// The picture's size on the board does not depend on the pixels: it is the frame's box.
async fn store_snapshot(app: &AppState, pixmap: Pixmap) -> Result<String, ApiError> {
    let store = std::sync::Arc::clone(&app.assets);
    let stored = tokio::task::spawn_blocking(move || -> Result<String, String> {
        let bytes = shrink(&pixmap, STORED_SIDE)
            .as_ref()
            .unwrap_or(&pixmap)
            .encode_png()
            .map_err(|error| error.to_string())?;
        Ok(store.put(&bytes)?.asset)
    })
    .await
    .map_err(|error| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;
    stored.map_err(|reason| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, reason))
}
