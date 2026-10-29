//! The three crops a mark bundle names — with the ink, the same crop without it, and the whole frame — built by
//! screenshotting the headless frame once per crop and compositing the mark's ink with core's raster.
//!
//! The ink is placed by core on exactly the layout the bridge reported, so the crop shows the mark where the
//! tablet draws it. The page is measured at scroll 0: document and viewport coordinates coincide, so a page rect
//! is `frame position + viewport rect`, and a screenshot clip is that viewport rect. Crops are served from a
//! small in-memory cache (`GET /marks` fills it, `GET /marks/<id>/image/…` reads it); an entry is stale as soon
//! as the page's `updated_seq` moves, so an edited page is rebuilt on the next read.

use std::collections::{BTreeMap, BTreeSet};

use axum::extract::{Path, State};
use axum::http::header;
use axum::response::{IntoResponse, Response};
use penwise_core::Bounds;
use penwise_core::item::{ItemId, ItemKind, LayerSet, Scene};
use penwise_core::raster::png::encode_png;
use penwise_core::raster::render_scene_onto;
use tiny_skia::Pixmap;

use crate::api::{ApiError, AppState};
use crate::inspect::{Clip, Inspect};
use crate::render::RenderError;

use super::AgentError;
use super::frame::FrameView;

/// Margin around a mark's ink in the `with_ink`/`without_ink` crop, in CSS px.
pub(crate) const CROP_MARGIN: f32 = 48.0;
/// Largest side of one crop, in px (`inspect` rejects anything thinner or larger).
const MAX_CROP_SIDE: i32 = 8000;
/// Which crop of a mark an image URL names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum ImageKind {
    /// Around the mark's ink, with the ink composited on the page.
    WithInk,
    /// The same crop of the page without the mark's ink.
    WithoutInk,
    /// The whole frame viewport, with the mark's ink.
    Context,
}

impl ImageKind {
    /// Every kind, in URL order.
    pub(crate) const ALL: [Self; 3] = [Self::WithInk, Self::WithoutInk, Self::Context];

    /// The URL/JSON name (`"with_ink"`, …), without the `.png`.
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::WithInk => "with_ink",
            Self::WithoutInk => "without_ink",
            Self::Context => "context",
        }
    }

    /// Inverse of [`ImageKind::as_str`]; accepts `with_ink.png` and the bare name.
    pub(crate) fn parse(text: &str) -> Option<Self> {
        let name = text.strip_suffix(".png").unwrap_or(text);
        Self::ALL.into_iter().find(|kind| kind.as_str() == name)
    }
}

/// A mark's three crops.
#[derive(Debug, Clone)]
pub(crate) struct BuiltImages {
    pub(crate) with_ink: Vec<u8>,
    pub(crate) without_ink: Vec<u8>,
    pub(crate) context: Vec<u8>,
}

impl BuiltImages {
    pub(crate) fn get(&self, kind: ImageKind) -> &[u8] {
        match kind {
            ImageKind::WithInk => &self.with_ink,
            ImageKind::WithoutInk => &self.without_ink,
            ImageKind::Context => &self.context,
        }
    }
}

/// One mark to crop: its effective page rect and the strokes that belong to it.
#[derive(Debug, Clone)]
pub(crate) struct MarkCrop {
    pub(crate) mark: ItemId,
    pub(crate) rect: [f32; 4],
    pub(crate) strokes: Vec<ItemId>,
}

/// Screenshots the crops of `crops` on `view`'s page and composites each mark's ink.
///
/// `ink` is the presentation scene (effective page coordinates) and `frame` is `[x, y, w, h]` of the frame's page
/// box, so a page rect is `frame position + viewport rect`.
///
/// # Errors
/// A screenshot, PNG decode/encode or raster failure.
pub(crate) async fn build(
    inspect: &Inspect,
    view: &FrameView,
    ink: &Scene,
    frame: [f32; 4],
    crops: &[MarkCrop],
) -> Result<BTreeMap<ItemId, BuiltImages>, AgentError> {
    let context = clip_rect([0.0, 0.0, frame[2], frame[3]])?;
    let around: BTreeMap<ItemId, [i32; 4]> = crops
        .iter()
        .map(|crop| {
            let doc = [
                crop.rect[0] - frame[0],
                crop.rect[1] - frame[1],
                crop.rect[2],
                crop.rect[3],
            ];
            Ok((crop.mark, clip_rect(inflate(doc, CROP_MARGIN))?))
        })
        .collect::<Result<_, AgentError>>()?;
    let mut wanted: BTreeSet<[i32; 4]> = around.values().copied().collect();
    wanted.insert(context);
    let mut shots: BTreeMap<[i32; 4], Pixmap> = BTreeMap::new();
    for rect in wanted {
        shots.insert(rect, screenshot(inspect, view, rect).await?);
    }
    let context_shot = shots
        .get(&context)
        .ok_or_else(|| AgentError::Internal("missing context crop".to_owned()))?;
    let mut out = BTreeMap::new();
    for crop in crops {
        let Some(&rect) = around.get(&crop.mark) else {
            continue;
        };
        let Some(shot) = shots.get(&rect) else {
            continue;
        };
        let mark_ink = mark_ink(ink, &crop.strokes)?;
        let with_ink_px = with_ink(shot, rect, frame, &mark_ink)?;
        let context_in = with_ink(context_shot, context, frame, &mark_ink)?;
        out.insert(
            crop.mark,
            BuiltImages {
                with_ink: encode(&with_ink_px)?,
                without_ink: encode(shot)?,
                context: encode(&context_in)?,
            },
        );
    }
    Ok(out)
}

/// `rect` (document px) inflated by `margin`.
fn inflate(rect: [f32; 4], margin: f32) -> [f32; 4] {
    [
        rect[0] - margin,
        rect[1] - margin,
        rect[2] + 2.0 * margin,
        rect[3] + 2.0 * margin,
    ]
}

/// The integer document rect of a float one (outward), clamped to what a screenshot may be.
fn clip_rect(doc: [f32; 4]) -> Result<[i32; 4], AgentError> {
    if doc.iter().any(|value| !value.is_finite()) {
        return Err(AgentError::Internal("crop has no finite bounds".to_owned()));
    }
    let left = doc[0].floor();
    let top = doc[1].floor();
    let width = (doc[0] + doc[2]).ceil() - left;
    let height = (doc[1] + doc[3]).ceil() - top;
    let clamp = |value: f32| -> i32 {
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_precision_loss,
            reason = "clamped to ±MAX_CROP_SIDE before the cast; 8000 is exact in f32"
        )]
        {
            value.clamp(-(MAX_CROP_SIDE as f32), MAX_CROP_SIDE as f32) as i32
        }
    };
    let rect = [
        clamp(left),
        clamp(top),
        clamp(width).max(1),
        clamp(height).max(1),
    ];
    if rect[2] <= 0 || rect[3] <= 0 {
        return Err(AgentError::Internal("empty crop".to_owned()));
    }
    Ok(rect)
}

/// One screenshot of `rect` (document px) as a pixmap.
async fn screenshot(
    inspect: &Inspect,
    view: &FrameView,
    rect: [i32; 4],
) -> Result<Pixmap, AgentError> {
    let clip = Clip::new(
        f64::from(rect[0]),
        f64::from(rect[1]),
        f64::from(rect[2]),
        f64::from(rect[3]),
    );
    let png = inspect.snapshot(&view.page, Some(clip)).await?;
    Pixmap::decode_png(&png).map_err(|error| AgentError::Internal(format!("crop decode: {error}")))
}

/// `shot` (document rect `rect`) with the mark's ink composited where core places it.
fn with_ink(
    shot: &Pixmap,
    rect: [i32; 4],
    frame: [f32; 4],
    ink: &Scene,
) -> Result<Pixmap, AgentError> {
    let mut canvas = shot.clone();
    let region = Bounds {
        min_x: frame[0] + px(rect[0]),
        min_y: frame[1] + px(rect[1]),
        max_x: frame[0] + px(rect[0]) + px(rect[2]),
        max_y: frame[1] + px(rect[1]) + px(rect[3]),
    };
    render_scene_onto(&mut canvas, ink, LayerSet::INK, &region, 1.0, &())
        .map_err(|error| AgentError::Render(RenderError::from(error)))?;
    Ok(canvas)
}

/// `rect` as an `f32` coordinate.
fn px(value: i32) -> f32 {
    #[allow(
        clippy::cast_precision_loss,
        reason = "crop coordinates are far below 2²⁴"
    )]
    {
        value as f32
    }
}

/// The presentation scene restricted to `strokes` (and the masks cutting them): only this mark's ink.
fn mark_ink(ink: &Scene, strokes: &[ItemId]) -> Result<Scene, AgentError> {
    let kept: BTreeSet<ItemId> = strokes.iter().copied().collect();
    let picked = ink
        .live_ids()
        .into_iter()
        .filter_map(|id| ink.get(id))
        .filter(|item| match &item.kind {
            ItemKind::Stroke(_) => kept.contains(&item.id),
            ItemKind::Erase(erase) => kept.contains(&erase.target),
            _ => false,
        });
    Scene::from_items(ink.kind(), picked.map(|item| (item.clone(), false)))
        .map_err(|error| AgentError::Internal(format!("mark ink: {error}")))
}

/// A crop as PNG.
fn encode(pixmap: &Pixmap) -> Result<Vec<u8>, AgentError> {
    encode_png(pixmap).map_err(|_| AgentError::Internal("png encode failed".to_owned()))
}

/// `GET /marks/<id>/image/<kind>.png` — a cached crop, rebuilt when the page changed since it was taken.
pub(crate) async fn route(
    State(app): State<AppState>,
    Path((id, kind)): Path<(String, String)>,
) -> Result<Response, ApiError> {
    let mark = ItemId::from_hex(&id)
        .ok_or_else(|| ApiError::bad_request("mark id is 32 hex characters"))?;
    let kind = ImageKind::parse(&kind)
        .ok_or_else(|| ApiError::bad_request("kind is with_ink, without_ink or context"))?;
    let page = app
        .storage
        .page_of_item(mark)
        .await?
        .ok_or_else(|| ApiError::not_found("unknown mark"))?;
    let view = app
        .storage
        .page_view(page)
        .await?
        .filter(|view| !view.page.deleted)
        .ok_or_else(|| ApiError::not_found("unknown mark"))?;
    if app.images.get(mark, kind, view.updated_seq).is_none() {
        super::scene::images_for(&app, &view, mark).await?;
    }
    let png = app
        .images
        .get(mark, kind, view.updated_seq)
        .ok_or_else(|| ApiError::not_found("no crop for that mark"))?;
    Ok((
        [
            (header::CONTENT_TYPE, "image/png"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        png.as_ref().clone(),
    )
        .into_response())
}
