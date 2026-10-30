//! The overview image of `look`: the region around the shown marks of one frame, each mark numbered.
//!
//! The crop is the union of the marks' rects inflated by the bundle crop margin, cut by the same
//! [`images::build`] the per-mark crops use (one synthetic crop whose rect is the union and whose ink is every
//! shown mark). The numbered badges are polylines ([`super::digits`]) rendered through core's raster onto the
//! decoded crop, then the whole is scaled down to at most [`MAX_SIDE`] pixels on its long side.

use penwise_core::Bounds;
use penwise_core::item::{ItemId, LayerSet, Scene};
use penwise_core::marks::MarkView;
use penwise_core::raster::png::encode_png;
use penwise_core::raster::render_scene_onto;
use tiny_skia::Pixmap;

use crate::annotate::{AnnotateRequest, Identity, to_items};
use crate::api::AppState;

use super::super::AgentError;
use super::super::frame::FrameView;
use super::super::images::{self, CROP_MARGIN, MarkCrop};
use super::super::scene::{frame_item, frame_keys, frame_position};
use super::super::shrink::shrink;
use super::digits;

/// Longest side of the image handed to the agent, in px.
pub(super) const MAX_SIDE: u32 = 1280;

/// One shown mark of the frame: its number in the text and where it is.
pub(super) struct Numbered {
    pub(super) number: usize,
    pub(super) mark: ItemId,
    pub(super) rect: [f32; 4],
    pub(super) strokes: Vec<ItemId>,
}

/// The overview PNG of `frame` for `marks`, or why it could not be made.
///
/// # Errors
/// The frame page could not be opened, a screenshot failed, or the raster refused.
pub(super) async fn overview(
    app: &AppState,
    scene: &Scene,
    frame: ItemId,
    marks: &[Numbered],
) -> Result<Vec<u8>, AgentError> {
    let (Some(item), Some(first)) = (frame_item(scene, frame), marks.first()) else {
        return Err(AgentError::Internal("nothing to show".to_owned()));
    };
    let position = frame_position(scene, frame);
    let keys = frame_keys(scene, frame);
    let browser =
        FrameView::open(app.inspect.as_ref(), &app.projects, frame, item, &keys, "").await?;
    let presentation = MarkView::new(scene, &browser.layouts, true).presentation();
    let ink = crate::render::ink_only(&presentation.scene)?;
    let union = union_rect(marks);
    let size = item.page_size();
    let crop = MarkCrop {
        mark: first.mark,
        rect: union,
        strokes: marks.iter().flat_map(|mark| mark.strokes.clone()).collect(),
    };
    let frame_box = [position[0], position[1], size[0], size[1]];
    let built = images::build(app.inspect.as_ref(), &browser, &ink, frame_box, &[crop]).await?;
    let shot = built
        .get(&first.mark)
        .ok_or_else(|| AgentError::Internal("no overview crop".to_owned()))?;
    let mut pixmap = Pixmap::decode_png(&shot.with_ink)
        .map_err(|error| AgentError::Internal(format!("overview decode: {error}")))?;
    let origin = [
        position[0] + (union[0] - position[0] - CROP_MARGIN).floor(),
        position[1] + (union[1] - position[1] - CROP_MARGIN).floor(),
    ];
    badge_marks(&mut pixmap, &ink, marks, origin)?;
    let scaled = shrink(&pixmap, MAX_SIDE);
    encode_png(scaled.as_ref().unwrap_or(&pixmap))
        .map_err(|_| AgentError::Internal("png encode failed".to_owned()))
}

/// The smallest rect holding every mark's rect.
fn union_rect(marks: &[Numbered]) -> [f32; 4] {
    let mut bounds = Bounds::EMPTY;
    for mark in marks {
        bounds = bounds.union(&Bounds {
            min_x: mark.rect[0],
            min_y: mark.rect[1],
            max_x: mark.rect[0] + mark.rect[2],
            max_y: mark.rect[1] + mark.rect[3],
        });
    }
    [
        bounds.min_x,
        bounds.min_y,
        bounds.max_x - bounds.min_x,
        bounds.max_y - bounds.min_y,
    ]
}

/// Draws badge `number` at the top-left corner of each mark, moving it down while it would sit on another.
pub(super) fn badge_marks(
    pixmap: &mut Pixmap,
    ink: &Scene,
    marks: &[Numbered],
    origin: [f32; 2],
) -> Result<(), AgentError> {
    // The image is scaled down afterwards; badges are drawn that much larger so they stay readable.
    let scale = (px(pixmap.width().max(pixmap.height())) / px(MAX_SIDE)).max(1.0);
    let mut placed: Vec<([f32; 2], f32)> = Vec::new();
    let mut strokes = Vec::new();
    for mark in marks {
        let radius = digits::radius(mark.number, scale);
        let mut center = [mark.rect[0] - radius * 0.5, mark.rect[1] - radius * 0.5];
        while placed.iter().any(|(other, other_radius)| {
            (other[0] - center[0]).hypot(other[1] - center[1]) < radius + other_radius
        }) {
            center[1] += radius * 2.0;
        }
        placed.push((center, radius));
        strokes.extend(digits::badge(mark.number, center, scale));
    }
    let request = AnnotateRequest {
        page: None,
        strokes,
    };
    let items = to_items(&request, 0, &mut Counter(0));
    let badges = Scene::from_items(ink.kind(), items.into_iter().map(|item| (item, false)))
        .map_err(|error| AgentError::Internal(format!("badges: {error}")))?;
    let region = Bounds {
        min_x: origin[0],
        min_y: origin[1],
        max_x: origin[0] + px(pixmap.width()),
        max_y: origin[1] + px(pixmap.height()),
    };
    render_scene_onto(pixmap, &badges, LayerSet::ALL, &region, 1.0, &())
        .map_err(|error| AgentError::Internal(format!("badges: {error}")))
}

#[allow(
    clippy::cast_precision_loss,
    reason = "pixmap sides are far below 2²⁴, exact in f32"
)]
fn px(side: u32) -> f32 {
    side as f32
}

/// Item ids and seeds from a counter, so the same marks always give the same pixels.
struct Counter(u64);

impl Identity for Counter {
    fn next(&mut self) -> (ItemId, u64) {
        self.0 += 1;
        let mut id = [0_u8; 16];
        id[8..].copy_from_slice(&self.0.to_be_bytes());
        (ItemId(id), self.0)
    }
}
