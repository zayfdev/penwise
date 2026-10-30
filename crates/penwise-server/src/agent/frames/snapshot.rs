//! `GET /frames/<id>/snapshot.png`: the frame's whole box as one PNG, for the agent and for a round's frozen
//! history picture.
//!
//! The box is cut into strips like a still ([`crate::frames::still::capture_strips`]) and stitched. The canvas is
//! bounded by pixel area: a frame too large for [`MAX_PIXELS`] is scaled down to fit instead of allocating
//! `css_w × height × 4` bytes. The response says what it did: `X-Penwise-Doc-Height` is the box height in CSS px
//! and `X-Penwise-Scale` the factor the image was scaled by (`1` unless it had to shrink).

use axum::extract::{Path, Query, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use penwise_core::Bounds;
use penwise_core::item::{FrameItem, ItemId};
use penwise_core::marks::MarkView;
use penwise_core::raster::png::encode_png;
use penwise_core::raster::render_scene_onto;
use serde::Deserialize;
use tiny_skia::{FilterQuality, Pixmap, PixmapPaint, Transform};

use crate::api::{ApiError, AppState};

use super::super::frame::FrameView;
use super::super::scene::{frame_item, frame_keys, frame_position};

/// The most pixels the stitched canvas may hold (160 MB of RGBA).
const MAX_PIXELS: f64 = 40_000_000.0;

/// `GET /frames/<id>/snapshot.png?ink=true|false`.
#[derive(Debug, Deserialize)]
pub(crate) struct SnapshotQuery {
    ink: Option<String>,
}

/// The frame's whole box as PNG (preset width × document height for a full-length frame), the owner's ink
/// composited when `ink` is not `false`/`0`/`no`.
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
    .map_err(unavailable)?;
    let [width, height] = snapshot_size(&frame, &browser);
    let strips =
        crate::frames::still::capture_strips(app.inspect.as_ref(), &browser.page, width, height)
            .await
            .map_err(unavailable)?;
    let scale = scale_for(width, height);
    let canvas = stitch(&strips, width, height, scale)?;
    let size = [px(width), px(height)];
    let png = if ink_enabled(query.ink.as_deref()) {
        composite(canvas, scene, &browser, position, size, scale)?
    } else {
        encode(&canvas)?
    };
    let mut response = (
        [
            (header::CONTENT_TYPE, "image/png"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        png,
    )
        .into_response();
    let headers = response.headers_mut();
    for (name, value) in [
        ("x-penwise-doc-height", height.to_string()),
        ("x-penwise-scale", scale.to_string()),
    ] {
        if let Ok(value) = HeaderValue::from_str(&value) {
            headers.insert(name, value);
        }
    }
    Ok(response)
}

/// The strips drawn one under the other onto a canvas of the (scaled) box size.
fn stitch(
    strips: &[crate::frames::still::Strip],
    width: u32,
    height: u32,
    scale: f32,
) -> Result<Pixmap, ApiError> {
    let mut canvas = Pixmap::new(scaled(width, scale), scaled(height, scale))
        .ok_or_else(|| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "snapshot too large"))?;
    let quality = if scale < 1.0 {
        FilterQuality::Bilinear
    } else {
        FilterQuality::Nearest
    };
    let paint = PixmapPaint {
        quality,
        ..PixmapPaint::default()
    };
    for strip in strips {
        let part = Pixmap::decode_png(&strip.png).map_err(|_| {
            ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "snapshot decode failed")
        })?;
        canvas.draw_pixmap(
            0,
            0,
            part.as_ref(),
            &paint,
            Transform::from_row(scale, 0.0, 0.0, scale, 0.0, px(strip.y) * scale),
            None,
        );
    }
    Ok(canvas)
}

fn unavailable(error: impl std::fmt::Display) -> ApiError {
    ApiError::new(StatusCode::SERVICE_UNAVAILABLE, error.to_string())
}

/// The snapshot's size in CSS px: the frame's box (the whole document for a full-length frame, as measured on
/// the pre-rolled page).
fn snapshot_size(frame: &FrameItem, browser: &FrameView) -> [u32; 2] {
    let [_, height] = frame.box_size(Some(browser.doc[1]));
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a box height is positive and far below 2³¹"
    )]
    let height = height.ceil().max(1.0) as u32;
    [frame.css_w, height]
}

/// The factor that brings a `width × height` canvas under [`MAX_PIXELS`] (`1` when it already fits).
fn scale_for(width: u32, height: u32) -> f32 {
    let area = f64::from(width) * f64::from(height);
    if area <= MAX_PIXELS {
        return 1.0;
    }
    #[allow(clippy::cast_possible_truncation, reason = "a factor in (0, 1)")]
    {
        (MAX_PIXELS / area).sqrt() as f32
    }
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a canvas side is positive and below 2³¹"
)]
fn scaled(side: u32, scale: f32) -> u32 {
    ((px(side) * scale).floor() as u32).max(1)
}

#[allow(
    clippy::cast_precision_loss,
    reason = "snapshot sides are far below 2²⁴"
)]
fn px(value: u32) -> f32 {
    value as f32
}

fn ink_enabled(ink: Option<&str>) -> bool {
    !matches!(ink, Some("false" | "0" | "no"))
}

/// The snapshot with every visible mark of the frame composited where core places it.
fn composite(
    mut pixmap: Pixmap,
    scene: &penwise_core::item::Scene,
    browser: &FrameView,
    position: [f32; 2],
    size: [f32; 2],
    scale: f32,
) -> Result<Vec<u8>, ApiError> {
    let presentation = MarkView::new(scene, &browser.layouts, false).presentation();
    let ink = crate::render::ink_only(&presentation.scene)
        .map_err(|error| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;
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
        scale,
        &(),
    )
    .map_err(|error| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;
    encode(&pixmap)
}

fn encode(pixmap: &Pixmap) -> Result<Vec<u8>, ApiError> {
    encode_png(pixmap)
        .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "png encode failed"))
}
