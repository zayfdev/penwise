//! Grouping on add-with-binding: a new stroke joins the newest matching mark on the frame, or starts one.

use std::collections::BTreeSet;

use super::{
    Binding, BindingMode, FrameLayouts, FrameState, MAX_MARK_STROKES, MarkItem, MarkStatus,
    MarkView, binding_origin, mark_of,
};
use crate::bounds::Bounds;
use crate::brush::BrushParams;
use crate::edit::{Edit, EditError, ItemPatch};
use crate::item::{Affine, Item, ItemId, ItemKind, Layer, Scene, StrokeItem};
use crate::sample::Sample;

/// Largest gap between two strokes of one mark, in ms.
pub const GROUP_GAP_MS: u64 = 1500;
/// Largest gap between two strokes' page boxes for them to join one mark, in CSS px.
pub const GROUP_DISTANCE_PX: f32 = 40.0;

/// A finished stroke drawn on a frame, in page coords, ready to bind and group.
#[derive(Debug, Clone)]
pub struct BoundStroke {
    /// The stroke's item id (minted by the caller; its ULID time is the grouping clock).
    pub id: ItemId,
    /// The id to use if a new mark has to be created.
    pub mark_id: ItemId,
    /// The frame it was drawn on.
    pub frame: ItemId,
    /// What the client asked for; [`BindingMode::Bound`] without a usable key becomes [`BindingMode::Area`].
    pub mode: BindingMode,
    pub key: Option<String>,
    /// z of the stroke and of a freshly created mark (from the caller's `Scene::next_z`).
    pub z: i64,
    pub mark_z: i64,
    pub layer: Layer,
    pub params: BrushParams,
    pub seed: u64,
    /// Quantized samples in page coords.
    pub samples: Vec<Sample>,
    pub now_ms: u64,
}

/// Binds `spec`'s samples to its frame and groups it: `[Add(stroke), Update(mark)]` or
/// `[Add(stroke), Add(new mark)]`, one edit blob = one undo step.
///
/// # Errors
/// [`EditError::InvalidValue`] naming `"frame"` when the frame is unknown or not a frame, `"binding"` for a bad
/// key/state, plus the usual item validation from the caller's `Add`.
pub fn add_bound_stroke(
    scene: &Scene,
    layouts: &FrameLayouts,
    spec: &BoundStroke,
) -> Result<Vec<Edit>, EditError> {
    let frame = frame_state(scene, layouts, spec.frame)
        .ok_or(EditError::InvalidValue { field: "frame" })?;
    let state = frame
        .layout
        .as_ref()
        .map_or_else(String::new, |layout| layout.state.clone());
    let (mode, key) = resolve_mode(&frame, spec.mode, spec.key.clone());
    let anchor = binding_origin(&frame, mode, key.as_deref()).unwrap_or(frame.pos);
    let binding = Binding::new(spec.frame, mode, frame.preset, key, state.clone(), anchor)?;
    let stroke = Item {
        id: spec.id,
        layer: spec.layer,
        z: spec.z,
        transform: Affine::IDENTITY,
        kind: ItemKind::Stroke(StrokeItem {
            params: spec.params,
            seed: spec.seed,
            samples: shift(&spec.samples, anchor),
            trim: None,
            binding: Some(binding),
        }),
    };
    let mut edits = vec![Edit::Add(stroke)];
    if let Some((mark, strokes)) = group_target(scene, layouts, spec, &frame) {
        edits.push(Edit::Update {
            id: mark,
            patch: ItemPatch {
                mark_strokes: Some(strokes),
                ..ItemPatch::default()
            },
        });
    } else {
        let mark = MarkItem {
            frame: spec.frame,
            preset: frame.preset,
            status: MarkStatus::New,
            state,
            strokes: vec![spec.id],
        };
        edits.push(Edit::Add(mark.item(spec.mark_id, spec.mark_z)));
    }
    Ok(edits)
}

/// Mark updates/removals after `edits` deleted strokes: a mark that lost members gets the shortened list, and a
/// mark that lost all of them is deleted in the same edit batch.
#[must_use]
pub fn prune_marks(scene: &Scene, edits: &[Edit]) -> Vec<Edit> {
    let mut gone = BTreeSet::new();
    for edit in edits {
        match edit {
            Edit::Delete(id)
            | Edit::Update {
                id,
                patch:
                    ItemPatch {
                        deleted: Some(true),
                        ..
                    },
            } => {
                gone.insert(*id);
            }
            Edit::Add(_) | Edit::Update { .. } => {}
        }
    }
    if gone.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for id in scene.live_ids() {
        if gone.contains(&id) {
            continue;
        }
        let Some(mark) = mark_of(scene, id) else {
            continue;
        };
        if !mark.strokes.iter().any(|stroke| gone.contains(stroke)) {
            continue;
        }
        let remaining: Vec<ItemId> = mark
            .strokes
            .iter()
            .copied()
            .filter(|stroke| !gone.contains(stroke))
            .collect();
        out.push(if remaining.is_empty() {
            Edit::Delete(id)
        } else {
            Edit::Update {
                id,
                patch: ItemPatch {
                    mark_strokes: Some(remaining),
                    ..ItemPatch::default()
                },
            }
        });
    }
    out
}

/// The frame's placement state, `None` when `frame` is not a live frame item.
fn frame_state(scene: &Scene, layouts: &FrameLayouts, frame: ItemId) -> Option<FrameState> {
    let item = scene.get(frame)?;
    let ItemKind::Frame(frame_item) = &item.kind else {
        return None;
    };
    Some(FrameState {
        pos: [item.transform.e, item.transform.f],
        size: frame_item.page_size(),
        preset: frame_item.preset,
        layout: layouts.get(frame).cloned(),
    })
}

/// Keeps `Bound` only when a key is present and its rect is known; otherwise the stroke is pinned to the area
/// (the bridge had no answer in time, or the element is gone).
fn resolve_mode(
    frame: &FrameState,
    mode: BindingMode,
    key: Option<String>,
) -> (BindingMode, Option<String>) {
    let key = key.filter(|k| !k.is_empty());
    if mode == BindingMode::Bound
        && key.is_some()
        && binding_origin(frame, BindingMode::Bound, key.as_deref()).is_some()
    {
        return (BindingMode::Bound, key);
    }
    (BindingMode::Area, None)
}

/// The newest matching mark and the member list the new stroke extends, if grouping rules allow it.
fn group_target(
    scene: &Scene,
    layouts: &FrameLayouts,
    spec: &BoundStroke,
    frame: &FrameState,
) -> Option<(ItemId, Vec<ItemId>)> {
    let mut newest: Option<(ItemId, ItemId)> = None;
    for id in scene.live_ids() {
        let Some(mark) = mark_of(scene, id) else {
            continue;
        };
        if mark.frame != spec.frame || mark.preset != frame.preset || mark.status.is_done() {
            continue;
        }
        let Some(last) = mark.strokes.last().copied() else {
            continue;
        };
        if newest.is_none_or(|(_, newer)| last > newer) {
            newest = Some((id, last));
        }
    }
    let (mark_id, last) = newest?;
    if spec.now_ms.saturating_sub(created_ms(last)) > GROUP_GAP_MS {
        return None;
    }
    let rect = MarkView::new(scene, layouts, true)
        .mark_entries()
        .into_iter()
        .find(|entry| entry.id == mark_id)?
        .rect?;
    let mark_box = Bounds {
        min_x: rect[0],
        min_y: rect[1],
        max_x: rect[0] + rect[2],
        max_y: rect[1] + rect[3],
    };
    if box_gap(&sample_bounds(&spec.samples), &mark_box) > GROUP_DISTANCE_PX {
        return None;
    }
    let mark = mark_of(scene, mark_id)?;
    let mut strokes = mark.strokes.clone();
    strokes.push(spec.id);
    (strokes.len() <= MAX_MARK_STROKES).then_some((mark_id, strokes))
}

/// ULID creation time of `id`, in ms (the first 48 bits, big-endian).
fn created_ms(id: ItemId) -> u64 {
    id.0[..6]
        .iter()
        .fold(0_u64, |acc, &byte| (acc << 8) | u64::from(byte))
}

/// Page box of a sample run (the samples are page coords at draw time).
fn sample_bounds(samples: &[Sample]) -> Bounds {
    samples.iter().fold(Bounds::EMPTY, |mut acc, sample| {
        acc.include_point(sample.x, sample.y);
        acc
    })
}

/// Distance between two closed boxes (`0` when they overlap, `∞` for an empty one).
fn box_gap(a: &Bounds, b: &Bounds) -> f32 {
    if a.is_empty() || b.is_empty() {
        return f32::INFINITY;
    }
    let dx = crate::float::max(crate::float::max(a.min_x - b.max_x, b.min_x - a.max_x), 0.0);
    let dy = crate::float::max(crate::float::max(a.min_y - b.max_y, b.min_y - a.max_y), 0.0);
    libm::hypotf(dx, dy)
}

/// Samples moved into binding-local coords, re-snapped to the codec grid.
fn shift(samples: &[Sample], origin: [f32; 2]) -> Vec<Sample> {
    samples
        .iter()
        .map(|sample| {
            Sample {
                x: sample.x - origin[0],
                y: sample.y - origin[1],
                pressure: sample.pressure,
                tilt: sample.tilt,
                azimuth: sample.azimuth,
                t_us: sample.t_us,
            }
            .quantize()
        })
        .collect()
}
