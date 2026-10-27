//! Pen tools: each builds core edits from gesture input, applies them locally (recorded in the open undo
//! gesture) and returns them for `SyncCore::commit_edits`.

use penwise_core::brush::{BrushFamily, BrushParams, Color};
use penwise_core::edit::{self, Edit, LassoParams};
use penwise_core::item::{
    Affine, ArrowHead, Item, ItemId, ItemKind, Layer, LayerSet, ShapeItem, ShapeKind, StrokeItem,
};
use penwise_core::sample::Sample;
use penwise_core::shape::resize::{create_frame, drag_endpoint, resize_edits, resize_frame};
use penwise_core::shape::{Handle, HandleFrame, regular_polygon};

use super::SceneHandle;
use crate::error::SyncError;

/// Floats per input sample: x, y, pressure, tilt, azimuth.
pub const SAMPLE_FLOATS: usize = 5;

/// A finished pen stroke to commit.
#[derive(Debug, Clone, Copy)]
pub struct StrokeSpec<'a> {
    /// The id used on the live deltas ([`SceneHandle::new_item_id`]).
    pub id: ItemId,
    /// `Preset::ALL` index, colour `0xAARRGGBB`, size scale: [`BrushParams::for_preset`].
    pub preset: usize,
    pub argb: u32,
    pub scale: f32,
    pub seed: u64,
    /// Interleaved raw samples (`SAMPLE_FLOATS` each) and one `t_us` per sample.
    pub samples: &'a [f32],
    pub times: &'a [i32],
}

/// A shape drag to commit (box shapes: opposite corners; line/arrow: endpoints).
#[derive(Debug, Clone, PartialEq)]
pub struct ShapeSpec {
    pub id: ItemId,
    pub kind: ShapeKind,
    pub start: [f32; 2],
    pub end: [f32; 2],
    pub constrain: bool,
    /// Outline brush (outline family).
    pub brush: BrushParams,
    pub fill: Option<Color>,
}

/// The outline brush a shape is drawn with: the preset's own when it is an outline brush, otherwise the first
/// preset (ballpoint) in the same colour and size scale, because core only accepts outline-family shape brushes.
#[must_use]
pub fn shape_brush(preset: usize, argb: u32, scale: f32) -> BrushParams {
    let brush = BrushParams::for_preset(preset, argb, scale);
    if matches!(brush.family, BrushFamily::Stamp(_)) {
        BrushParams::for_preset(0, argb, scale)
    } else {
        brush
    }
}

/// `[x, y]` pairs from interleaved floats (a trailing odd float is dropped).
#[must_use]
pub fn pairs(flat: &[f32]) -> Vec<[f32; 2]> {
    flat.chunks_exact(2).map(|p| [p[0], p[1]]).collect()
}

/// Shape kind from its frozen code (`ShapeKind::code`) and one numeric parameter: corner radius for rounded
/// rectangles, arrow head (0 end, 1 both) for arrows, side count for regular polygons.
///
/// # Errors
/// Unknown code or head.
pub fn shape_kind(code: i32, param: f32) -> Result<ShapeKind, SyncError> {
    Ok(match code {
        0 => ShapeKind::Rect,
        1 => ShapeKind::RoundedRect { radius: param },
        2 => ShapeKind::Ellipse,
        3 => ShapeKind::Line,
        4 => {
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "head code 0/1"
            )]
            let head = ArrowHead::from_code(param.clamp(0.0, 255.0) as u8)
                .ok_or(SyncError::BadArgument("arrow head"))?;
            ShapeKind::Arrow { head }
        }
        5 => {
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "clamped side count"
            )]
            let sides = param.clamp(3.0, 64.0) as u32;
            regular_polygon(sides)
        }
        _ => return Err(SyncError::BadArgument("shape kind")),
    })
}

/// A resize/drag handle from its enum index (`Handle` declaration order).
///
/// # Errors
/// Unknown index.
pub fn handle_from_code(code: i32) -> Result<Handle, SyncError> {
    const ALL: [Handle; 12] = [
        Handle::TopLeft,
        Handle::Top,
        Handle::TopRight,
        Handle::Right,
        Handle::BottomRight,
        Handle::Bottom,
        Handle::BottomLeft,
        Handle::Left,
        Handle::Rotate,
        Handle::Start,
        Handle::End,
        Handle::Body,
    ];
    usize::try_from(code)
        .ok()
        .and_then(|i| ALL.get(i).copied())
        .ok_or(SyncError::BadArgument("handle"))
}

impl SceneHandle {
    /// Commits a finished stroke: `Add(Item::Stroke)` on the ink layer at the next z, samples quantized to the
    /// codec grid first (so the committed geometry equals the live one).
    ///
    /// # Errors
    /// Mismatched/short arrays, z exhaustion or a value the scene rejects.
    pub fn add_stroke(&mut self, spec: &StrokeSpec<'_>) -> Result<Vec<Edit>, SyncError> {
        if spec.samples.len() != spec.times.len() * SAMPLE_FLOATS || spec.times.is_empty() {
            return Err(SyncError::BadArgument("samples and times disagree"));
        }
        let samples = spec
            .samples
            .chunks_exact(SAMPLE_FLOATS)
            .zip(spec.times)
            .map(|(s, &t)| {
                Sample {
                    x: s[0],
                    y: s[1],
                    pressure: s[2],
                    tilt: s[3],
                    azimuth: s[4],
                    t_us: t.cast_unsigned(),
                }
                .quantize()
            })
            .collect();
        let item = Item {
            id: spec.id,
            layer: Layer::Ink,
            z: self.next_z()?,
            transform: Affine::IDENTITY,
            kind: ItemKind::Stroke(StrokeItem {
                params: BrushParams::for_preset(spec.preset, spec.argb, spec.scale),
                seed: spec.seed,
                samples,
                trim: None,
            }),
        };
        self.local(vec![Edit::Add(item)])
    }

    /// Commits a shape from a drag: frame from [`create_frame`], `Add(Item::Shape)`.
    ///
    /// # Errors
    /// z exhaustion or a value the scene rejects (e.g. a non-outline brush).
    pub fn add_shape(&mut self, spec: &ShapeSpec) -> Result<Vec<Edit>, SyncError> {
        let frame = create_frame(spec.start, spec.end, spec.kind.is_linear(), spec.constrain);
        let item = Item {
            id: spec.id,
            layer: Layer::Ink,
            z: self.next_z()?,
            transform: frame.transform,
            kind: ItemKind::Shape(ShapeItem {
                kind: spec.kind.clone(),
                size: frame.size,
                brush: spec.brush,
                fill: spec.fill,
            }),
        };
        self.local(vec![Edit::Add(item)])
    }

    /// Resizes/re-aims shape `id` by dragging `handle` to `pointer` (empty: not a live shape or no change).
    ///
    /// # Errors
    /// The scene refused the resulting edit.
    pub fn resize_shape(
        &mut self,
        id: ItemId,
        handle: Handle,
        pointer: [f32; 2],
        constrain: bool,
    ) -> Result<Vec<Edit>, SyncError> {
        let Some(item) = self.scene.get(id) else {
            return Ok(Vec::new());
        };
        let ItemKind::Shape(shape) = &item.kind else {
            return Ok(Vec::new());
        };
        let frame = HandleFrame::of_shape(shape, &item.transform);
        let next = match handle {
            Handle::Start | Handle::End => drag_endpoint(&frame, handle, pointer, constrain),
            _ => resize_frame(&frame, handle, pointer, constrain),
        };
        let edits = resize_edits(&self.scene, id, &next);
        self.local(edits)
    }

    /// One eraser chunk along `path`: whole-item (`precise = false`) or exact (`precise = true`). Call once
    /// per chunk inside a gesture; every chunk's edits join the same undo step.
    ///
    /// # Errors
    /// The scene refused an edit.
    pub fn erase(
        &mut self,
        path: &[[f32; 2]],
        radius: f32,
        precise: bool,
        layers: LayerSet,
    ) -> Result<Vec<Edit>, SyncError> {
        let edits = if precise {
            edit::erase_precise(&self.scene, path, radius, layers, &mut self.ids)
        } else {
            edit::erase_strokes(&self.scene, path, radius, layers)
        };
        self.local(edits)
    }

    /// Items selected by a lasso polygon, bottom → top.
    #[must_use]
    pub fn lasso(&self, polygon: &[[f32; 2]], layers: LayerSet) -> Vec<ItemId> {
        edit::lasso(&self.scene, polygon, layers, &LassoParams::DEFAULT)
    }

    /// Topmost-last hits at a tap.
    #[must_use]
    pub fn hit_test(&self, point: [f32; 2], tolerance: f32, layers: LayerSet) -> Vec<ItemId> {
        edit::hit_test(&self.scene, point, tolerance, layers)
    }

    /// Deletes `ids` (lasso delete).
    ///
    /// # Errors
    /// The scene refused an edit.
    pub fn delete_items(&mut self, ids: &[ItemId]) -> Result<Vec<Edit>, SyncError> {
        let edits = edit::select::delete_items(&self.scene, ids);
        self.local(edits)
    }

    /// Moves `ids` by `(dx, dy)` (absolute new transforms).
    ///
    /// # Errors
    /// The scene refused an edit.
    pub fn move_items(&mut self, ids: &[ItemId], dx: f32, dy: f32) -> Result<Vec<Edit>, SyncError> {
        let edits = edit::transform::move_items(&self.scene, ids, dx, dy);
        self.local(edits)
    }

    /// Moves `ids` to `layer` (e.g. promote agent marks to ink).
    ///
    /// # Errors
    /// The scene refused an edit.
    pub fn set_layer(&mut self, ids: &[ItemId], layer: Layer) -> Result<Vec<Edit>, SyncError> {
        let edits = edit::select::set_layer(&self.scene, ids, layer);
        self.local(edits)
    }

    fn next_z(&self) -> Result<i64, SyncError> {
        self.scene
            .next_z()
            .ok_or(SyncError::BadArgument("page z range exhausted"))
    }
}
