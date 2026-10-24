//! Precise erase of outline strokes: split in the raw-sample domain (editing-algorithms.md §1, option A).
//!
//! Owner: slice b.
//!
//! Coverage rule: origin point `u` (centre `c(u)`, half-width `h(u)`, both lerped) is erased when
//! `dist(c(u), E) < r_e + h(u)`, `E` the eraser centreline. Cut params come from 12 fixed bisection steps
//! on each inside/outside boundary segment (deterministic; 1/4096 segment = one [`SampleParam`] frac step).
//! Segments longer than `r_e / 2` are tested at sub-steps so a thin eraser cannot pass between samples.

mod coverage;

use super::Edit;
use super::erase_stroke::{EraserIndex, eraser_bounds};
use crate::item::{
    Affine, CONTEXT_SAMPLES, CentrePoint, Centreline, IdSource, Item, ItemKind, SampleParam,
    SampleRange, StrokeItem, Trim,
};
use coverage::Coverage;

/// Number of bisection steps per cut (frozen with [`SampleParam::FRAC_BITS`]).
pub const CUT_BISECTION_STEPS: u32 = 12;

/// Pieces shorter than this fraction of their mean half-width (visible arc length) are dropped.
pub const MIN_PIECE_LENGTH_FACTOR: f32 = 0.5;

/// `visible` minus the eraser coverage, in origin space. `visible` is the stroke's effective visible set
/// (sorted, disjoint); coverage is evaluated on the stroke's page centreline (`stroke.centreline` under
/// `transform`, all stored samples). Ends created by the eraser are marked cut; surviving ends keep their
/// flags. Returns `None` when nothing visible is covered (the stroke is untouched).
#[must_use]
pub fn uncovered_ranges(
    stroke: &StrokeItem,
    transform: &Affine,
    visible: &[SampleRange],
    eraser: &[[f32; 2]],
    radius: f32,
) -> Option<Vec<SampleRange>> {
    eraser_bounds(eraser, radius)?;
    uncovered_in(
        stroke,
        transform,
        visible,
        &EraserIndex::new(eraser),
        radius,
    )
}

/// [`uncovered_ranges`] with a prebuilt index of a validated eraser path.
pub(crate) fn uncovered_in(
    stroke: &StrokeItem,
    transform: &Affine,
    visible: &[SampleRange],
    index: &EraserIndex,
    radius: f32,
) -> Option<Vec<SampleRange>> {
    let points = stroke.centre_samples(transform);
    let cov = Coverage {
        stroke,
        points: &points,
        index,
        radius,
    };
    let mut kept = Vec::new();
    let mut params = Vec::new();
    let mut touched = false;
    for range in visible {
        let Some(clipped) = stroke.clip(range) else {
            continue;
        };
        stroke.range_params(&clipped, &mut params);
        touched |= cov.split_range(&clipped, &params, &mut kept);
    }
    touched.then_some(kept)
}

/// Edits replacing live stroke `item` (`id = item.id`, `stroke` = its data) by pieces over `kept` (output of [`uncovered_ranges`]):
/// `Delete(id)` then one `Add` per kept range whose visible arc length ≥ [`MIN_PIECE_LENGTH_FACTOR`] ·
/// mean half-width, in range order, each with `ids.next_id()`. A piece copies the original's layer, z,
/// transform, params and seed; its samples are the origin window `[start.index − CONTEXT_SAMPLES,
/// end.index + 1 + CONTEXT_SAMPLES]` clipped to what `id` stores; its [`crate::item::Trim`] has
/// `origin = stroke.origin(id)`, `visible` = the member's family set with its own range replaced by the kept
/// pieces (other members' ranges untouched), and `own` = the piece's index in that set. Only `Delete(id)` when
/// nothing survives.
#[must_use]
pub fn split_edits(
    item: &Item,
    stroke: &StrokeItem,
    kept: &[SampleRange],
    ids: &mut dyn IdSource,
) -> Vec<Edit> {
    let id = item.id;
    let mut edits = vec![Edit::Delete(id)];
    let mut centreline = Centreline::default();
    let survivors: Vec<SampleRange> = kept
        .iter()
        .copied()
        .filter(|r| {
            stroke.centreline(&item.transform, &[*r], &mut centreline);
            long_enough(&centreline.points)
        })
        .collect();
    if survivors.is_empty() {
        return edits;
    }
    let own_index = stroke
        .trim
        .as_ref()
        .and_then(|t| usize::try_from(t.own).ok())
        .unwrap_or(0);
    let mut set = Vec::new();
    for (k, r) in stroke.family_visible().into_iter().enumerate() {
        if k == own_index {
            set.extend_from_slice(&survivors);
        } else {
            set.push(r);
        }
    }
    for (j, range) in survivors.iter().enumerate() {
        let own = u32::try_from(own_index + j).unwrap_or(u32::MAX);
        let trim = Trim {
            origin: stroke.origin(id),
            sample_offset: 0,
            visible: set.clone(),
            own,
        };
        if let Some(piece) = piece(stroke, range, trim) {
            edits.push(Edit::Add(Item {
                id: ids.next_id(),
                layer: item.layer,
                z: item.z,
                transform: item.transform,
                kind: ItemKind::Stroke(piece),
            }));
        }
    }
    edits
}

/// Bisection of one boundary: the param in `[a, b]` (one origin segment) where coverage flips, after
/// [`CUT_BISECTION_STEPS`] halvings, rounded toward the uncovered side. `a` when `a ≥ b`.
#[must_use]
pub fn bisect_cut(
    stroke: &StrokeItem,
    transform: &Affine,
    a: SampleParam,
    b: SampleParam,
    eraser: &[[f32; 2]],
    radius: f32,
) -> SampleParam {
    if a >= b || eraser_bounds(eraser, radius).is_none() {
        return a;
    }
    let points = stroke.centre_samples(transform);
    Coverage {
        stroke,
        points: &points,
        index: &EraserIndex::new(eraser),
        radius,
    }
    .bisect(a, b)
}

/// Piece data for `range`: the context window of stored samples around it, with `trim` (whose
/// `sample_offset` is filled in here). `None` when the window is empty (range outside the stored samples).
fn piece(stroke: &StrokeItem, range: &SampleRange, mut trim: Trim) -> Option<StrokeItem> {
    let offset = stroke.sample_offset();
    let lo = range.start.index().saturating_sub(CONTEXT_SAMPLES);
    let lo = if lo < offset { offset } else { lo };
    let hi = range
        .end
        .index()
        .saturating_add(1)
        .saturating_add(CONTEXT_SAMPLES);
    let last = stroke.last_index();
    let hi = if hi > last { last } else { hi };
    let from = usize::try_from(lo.checked_sub(offset)?).ok()?;
    let to = usize::try_from(hi.checked_sub(offset)?).ok()?;
    let samples = stroke.samples.get(from..=to)?.to_vec();
    trim.sample_offset = lo;
    Some(StrokeItem {
        params: stroke.params,
        seed: stroke.seed,
        samples,
        trim: Some(trim),
    })
}

/// Visible arc length ≥ [`MIN_PIECE_LENGTH_FACTOR`] · mean half-width.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "f64 accumulators rounded once; point counts are far below 2^52"
)]
fn long_enough(points: &[CentrePoint]) -> bool {
    if points.is_empty() {
        return false;
    }
    let mut arc = 0.0f64;
    let mut width = 0.0f64;
    for w in points.windows(2) {
        let dx = f64::from(w[1].x) - f64::from(w[0].x);
        let dy = f64::from(w[1].y) - f64::from(w[0].y);
        arc += (dx * dx + dy * dy).sqrt();
    }
    for p in points {
        width += f64::from(p.half_width);
    }
    let mean = (width / points.len() as f64) as f32;
    arc as f32 >= MIN_PIECE_LENGTH_FACTOR * mean
}
